use grove_core::db;
use grove_core::events;
use grove_core::events::writer_queue::{PendingEvent, WriterQueue};
use tempfile::TempDir;

fn setup_db_with_run() -> (TempDir, String) {
    let dir = TempDir::new().unwrap();
    db::initialize(dir.path()).unwrap();

    let conn = db::DbHandle::new(dir.path()).connect().unwrap();
    let run_id = "run_contention";
    conn.execute(
        "INSERT INTO runs(id,objective,state,budget_usd,cost_used_usd,created_at,updated_at)
         VALUES(?1,'test','executing',1.0,0,'2024-01-01T00:00:00Z','2024-01-01T00:00:00Z')",
        [run_id],
    )
    .unwrap();
    (dir, run_id.to_string())
}

#[test]
fn concurrent_writes_via_writer_queue_all_succeed() {
    let (dir, run_id) = setup_db_with_run();
    let db_path = db::db_path(dir.path());

    let handles: Vec<_> = (0..4_u32)
        .map(|thread_id| {
            let path = db_path.clone();
            let rid = run_id.clone();
            std::thread::spawn(move || {
                let mut conn = grove_core::db::connection::open(&path).unwrap();
                let mut wq = WriterQueue::new();
                for i in 0..10_u32 {
                    wq.push(PendingEvent {
                        run_id: rid.clone(),
                        session_id: None,
                        event_type: format!("thread_{thread_id}_event_{i}"),
                        payload_json: "{}".to_string(),
                        created_at: chrono::Utc::now().to_rfc3339(),
                    });
                }
                wq.flush(&mut conn).unwrap();
            })
        })
        .collect();

    for handle in handles {
        handle.join().expect("thread should not panic");
    }

    let conn = db::DbHandle::new(dir.path()).connect().unwrap();
    let all_events = events::list_for_run(&conn, &run_id).unwrap();
    assert_eq!(
        all_events.len(),
        40,
        "all 40 events from 4×10 threads must be present; got {}",
        all_events.len()
    );
}

#[test]
fn concurrent_direct_emits_all_succeed() {
    let (dir, run_id) = setup_db_with_run();
    let db_path = db::db_path(dir.path());

    let handles: Vec<_> = (0..4_u32)
        .map(|t| {
            let path = db_path.clone();
            let rid = run_id.clone();
            std::thread::spawn(move || {
                let conn = grove_core::db::connection::open(&path).unwrap();
                for i in 0..10_u32 {
                    events::emit(
                        &conn,
                        &rid,
                        None,
                        &format!("direct_{t}_{i}"),
                        serde_json::json!({}),
                    )
                    .unwrap();
                }
            })
        })
        .collect();

    for handle in handles {
        handle.join().expect("thread should not panic");
    }

    let conn = db::DbHandle::new(dir.path()).connect().unwrap();
    let all_events = events::list_for_run(&conn, &run_id).unwrap();
    assert_eq!(all_events.len(), 40);
}
