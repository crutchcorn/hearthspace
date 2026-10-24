use super::*;
use std::os::unix::net::UnixListener;

fn exchange(response: &[u8], limit: usize) -> io::Result<Vec<u8>> {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("control.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let response = response.to_vec();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0; 8];
        stream.read_exact(&mut request).unwrap();
        assert_eq!(&request, b"windows\n");
        let _ = stream.write_all(&response);
    });
    let reply = request_payload(&path, "windows", Duration::from_secs(1), limit);
    server.join().unwrap();
    reply
}

#[test]
fn coalesced_header_keeps_body_and_respects_zero_length() {
    assert_eq!(exchange(b"ok 5\nhello", 5).unwrap(), b"hello");
    assert_eq!(exchange(b"ok 0\n", 0).unwrap(), b"");
}

#[test]
fn malformed_headers_and_oversize_payloads_fail_before_reading_body() {
    for bytes in [
        b"ok 6\n".as_slice(),
        b"ok +1\n",
        b"ok -1\n",
        b"ok NaN\n",
        b"okay 1\n",
        b"ok 99999999999999999999999999\n",
        b"ok \xff\n",
    ] {
        assert_eq!(
            exchange(bytes, 5).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }
    assert_eq!(
        exchange(&vec![b'x'; 4096], 5).unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
    assert_eq!(
        exchange(b"ok 5\nhi", 5).unwrap_err().kind(),
        io::ErrorKind::UnexpectedEof
    );
    assert_eq!(
        exchange(b"ok", 5).unwrap_err().kind(),
        io::ErrorKind::UnexpectedEof
    );
    assert!(
        exchange(b"err session is locked\n", 5)
            .unwrap_err()
            .to_string()
            .contains("session is locked")
    );
}

#[test]
fn malformed_request_never_connects() {
    for command in ["", "windows\nquit", "windows\rquit"] {
        assert_eq!(
            request_payload(Path::new("/absent"), command, Duration::from_secs(1), 10)
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
        );
    }
}

#[test]
fn delayed_header_and_body_share_one_deadline() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("control.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0; 8];
        stream.read_exact(&mut request).unwrap();
        for byte in b"ok 5\nhello" {
            if stream.write_all(&[*byte]).is_err() {
                break;
            }
            thread::sleep(Duration::from_millis(15));
        }
    });
    let start = Instant::now();
    assert_eq!(
        request_payload(&path, "windows", Duration::from_millis(100), 5)
            .unwrap_err()
            .kind(),
        io::ErrorKind::TimedOut
    );
    assert!(start.elapsed() < Duration::from_secs(1));
    server.join().unwrap();
}
