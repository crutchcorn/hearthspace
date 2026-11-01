use crate::state::{Data, Input, Snapshot, State};
use anyhow::{Result, anyhow};
use std::{
    os::unix::net::UnixStream,
    sync::{Arc, mpsc},
    thread,
    time::Duration,
};
use wayland_server::Display;

type Reply = mpsc::Sender<Result<Snapshot, String>>;

pub struct Server {
    tx: mpsc::Sender<(Input, Reply)>,
    thread: Option<thread::JoinHandle<Result<(), String>>>,
}

impl Server {
    pub fn start(socket: UnixStream) -> Result<Self> {
        let (tx, rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::channel();
        let thread = thread::spawn(move || {
            let result = serve(socket, rx, &ready_tx).map_err(|e| format!("{e:#}"));
            if let Err(error) = &result {
                let _ = ready_tx.send(Err(error.clone()));
            }
            result
        });
        ready_rx
            .recv_timeout(Duration::from_secs(5))?
            .map_err(|e| anyhow!(e))?;
        Ok(Self {
            tx,
            thread: Some(thread),
        })
    }

    pub fn send(&self, input: Input) -> Result<Snapshot> {
        let (tx, rx) = mpsc::channel();
        self.tx.send((input, tx))?;
        rx.recv_timeout(Duration::from_secs(3))?
            .map_err(|e| anyhow!(e))
    }

    pub fn stop(mut self) -> Result<()> {
        let _ = self.send(Input::Stop);
        self.thread
            .take()
            .unwrap()
            .join()
            .map_err(|_| anyhow!("Wayland server panicked"))?
            .map_err(|e| anyhow!(e))
    }
}

fn serve(
    socket: UnixStream,
    rx: mpsc::Receiver<(Input, Reply)>,
    ready: &mpsc::Sender<Result<(), String>>,
) -> Result<()> {
    let mut display = Display::<State>::new()?;
    let mut handle = display.handle();
    let mut state = State::new(&handle)?;
    let _client = handle.insert_client(socket, Arc::new(Data::default()))?;
    ready.send(Ok(()))?;
    loop {
        display.dispatch_clients(&mut state)?;
        match rx.try_recv() {
            Ok((input, reply)) => {
                let stop = matches!(input, Input::Stop);
                let result = state
                    .input(input)
                    .and_then(|_| {
                        display.flush_clients()?;
                        Ok(state.snapshot())
                    })
                    .map_err(|e| format!("{e:#}"));
                let _ = reply.send(result);
                if stop {
                    return Ok(());
                }
            }
            Err(mpsc::TryRecvError::Disconnected) => return Ok(()),
            Err(mpsc::TryRecvError::Empty) => {}
        }
        display.flush_clients()?;
        thread::sleep(Duration::from_millis(1));
    }
}
