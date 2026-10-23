use super::*;
use smithay::reexports::calloop::EventLoop;
use std::{fs, sync::Mutex};

// These tests exercise the process-wide slot limit, so serialize only this group.
static TESTS: Mutex<()> = Mutex::new(());

fn response<D>(
    events: &mut EventLoop<'static, D>,
    data: &mut D,
    receive: &mpsc::Receiver<ControlReply>,
) -> ControlReply {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if let Ok(reply) = receive.try_recv() {
            return reply;
        }
        assert!(Instant::now() < deadline, "missing deferred reply");
        events
            .dispatch(Some(Duration::from_millis(10)), data)
            .unwrap();
    }
}

fn workers_finished() {
    let deadline = Instant::now() + Duration::from_secs(2);
    while WORKERS.load(Ordering::Acquire) != 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(WORKERS.load(Ordering::Acquire), 0);
}

#[test]
fn stalled_preparation_keeps_input_live_times_out_and_cannot_launch_late() {
    let _test = TESTS.lock().unwrap();
    let mut events = EventLoop::<(usize, usize)>::try_new().unwrap();
    let mut state = (0, 0);
    let (entered, ready) = mpsc::channel();
    let (release, stalled) = mpsc::channel();
    let ControlReply::Deferred(receive) = start(
        &events.handle(),
        Duration::from_millis(40),
        move || {
            entered.send(()).unwrap();
            stalled.recv_timeout(Duration::from_secs(2)).unwrap();
            Ok(())
        },
        |(), _, state| {
            state.0 += 1;
            Ok(Command::new("/bin/true"))
        },
    ) else {
        panic!("expected deferred preparation")
    };
    ready.recv_timeout(Duration::from_secs(1)).unwrap();
    events
        .handle()
        .insert_source(Timer::immediate(), |_, _, state| {
            state.1 += 1;
            TimeoutAction::Drop
        })
        .unwrap();
    let ControlReply::Err(error) = response(&mut events, &mut state, &receive) else {
        panic!("stalled probe must time out")
    };
    assert!(error.contains("timed out"), "{error}");
    assert_eq!(
        state,
        (0, 1),
        "unrelated event must run while preparation blocks"
    );
    assert_eq!(WORKERS.load(Ordering::Acquire), 1);
    release.send(()).unwrap();
    workers_finished();
    events.dispatch(Some(Duration::ZERO), &mut state).unwrap();
    assert_eq!(
        state.0, 0,
        "expired preparation must not reach launch configuration"
    );
}

#[test]
fn expired_stalled_workers_retain_slots_and_cannot_create_unbounded_replacements() {
    let _test = TESTS.lock().unwrap();
    let mut events = EventLoop::<usize>::try_new().unwrap();
    let mut configured = 0;
    let mut releases = Vec::new();
    let mut replies = Vec::new();
    for _ in 0..MAX_WORKERS {
        let (release, stalled) = mpsc::channel();
        let ControlReply::Deferred(receive) = start(
            &events.handle(),
            Duration::from_millis(40),
            move || {
                stalled.recv_timeout(Duration::from_secs(2)).unwrap();
                Ok(())
            },
            |(), _, configured| {
                *configured += 1;
                Ok(Command::new("/bin/true"))
            },
        ) else {
            panic!("slot should be available")
        };
        releases.push(release);
        replies.push(receive);
    }
    let extra = || {
        start(
            &events.handle(),
            Duration::from_secs(1),
            || Ok(()),
            |(), _, _| Ok(Command::new("/bin/true")),
        )
    };
    assert!(matches!(extra(), ControlReply::Err(error) if error.contains("busy")));
    for receive in replies {
        assert!(matches!(
            response(&mut events, &mut configured, &receive),
            ControlReply::Err(_)
        ));
    }
    assert_eq!(WORKERS.load(Ordering::Acquire), MAX_WORKERS);
    assert!(
        matches!(start(&events.handle(), Duration::from_secs(1), || Ok(()), |(), _, _| Ok(Command::new("/bin/true"))), ControlReply::Err(error) if error.contains("busy"))
    );
    for release in releases {
        release.send(()).unwrap();
    }
    workers_finished();
    events
        .dispatch(Some(Duration::ZERO), &mut configured)
        .unwrap();
    assert_eq!(configured, 0);
}

#[test]
fn dropping_the_event_loop_cancels_prepared_work_without_joining_blocked_io() {
    let _test = TESTS.lock().unwrap();
    let events = EventLoop::<()>::try_new().unwrap();
    let (release, stalled) = mpsc::channel();
    let ControlReply::Deferred(receive) = start(
        &events.handle(),
        Duration::from_secs(1),
        move || {
            stalled.recv_timeout(Duration::from_secs(2)).unwrap();
            Ok(())
        },
        |(), _, _| panic!("cancelled preparation must not launch"),
    ) else {
        panic!("expected deferred preparation")
    };
    drop(events);
    assert!(matches!(
        receive.try_recv(),
        Err(mpsc::TryRecvError::Disconnected)
    ));
    release.send(()).unwrap();
    workers_finished();
}

#[test]
fn launcher_failure_reaches_the_deferred_reply() {
    let _test = TESTS.lock().unwrap();
    let mut events = EventLoop::<()>::try_new().unwrap();
    let ControlReply::Deferred(receive) = start(
        &events.handle(),
        Duration::from_secs(1),
        || Ok(()),
        |(), _, _| Ok(Command::new("/bin/false")),
    ) else {
        panic!("launch should be deferred")
    };
    let ControlReply::Err(error) = response(&mut events, &mut (), &receive) else {
        panic!("launcher failure must be reported")
    };
    assert!(error.contains("application launcher failed"), "{error}");
    workers_finished();
}

#[test]
fn a_disconnected_caller_does_not_leave_a_child_or_zombie() {
    let _test = TESTS.lock().unwrap();
    let root = tempfile::tempdir().unwrap();
    let ready = root.path().join("ready");
    let release = root.path().join("release");
    let mut command = Command::new("/bin/sh");
    command
        .args([
            "-c",
            "echo $$ > \"$1.pid\"; touch \"$1\"; while [ ! -e \"$2\" ]; do sleep 0.01; done",
            "fixture",
        ])
        .arg(&ready)
        .arg(&release);
    let mut events = EventLoop::<()>::try_new().unwrap();
    let ControlReply::Deferred(receive) = start(
        &events.handle(),
        Duration::from_secs(2),
        move || Ok(command),
        |command, _, _| Ok(command),
    ) else {
        panic!("launch should be deferred")
    };
    let deadline = Instant::now() + Duration::from_secs(1);
    while !ready.exists() && Instant::now() < deadline {
        events
            .dispatch(Some(Duration::from_millis(10)), &mut ())
            .unwrap();
    }
    assert!(ready.exists());
    let pid: u32 = fs::read_to_string(ready.with_extension("pid"))
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    drop(receive);
    fs::write(release, []).unwrap();
    workers_finished();
    assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
    events.dispatch(Some(Duration::ZERO), &mut ()).unwrap();
}
