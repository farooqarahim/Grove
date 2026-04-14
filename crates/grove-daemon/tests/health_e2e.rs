use grove_daemon::config::DaemonConfig;
use grove_daemon::server::serve;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::time::{Duration, Instant};
use tempfile::tempdir;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn health_over_socket() {
    let tmp = tempdir().unwrap();
    let cfg = DaemonConfig::from_project_root(tmp.path()).unwrap();
    let sock = cfg.socket_path.clone();

    let handle = tokio::spawn(async move {
        let _ = serve(cfg).await;
    });

    let start = Instant::now();
    while !sock.exists() {
        if start.elapsed() > Duration::from_secs(2) {
            panic!("socket never appeared at {sock:?}");
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }

    let stream = UnixStream::connect(&sock).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut writer = stream.try_clone().unwrap();
    writer
        .write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"grove.health\",\"params\":{},\"id\":1}\n")
        .unwrap();

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    let resp: serde_json::Value = serde_json::from_str(line.trim()).unwrap();
    assert_eq!(resp["result"]["status"], "ok");

    handle.abort();
}
