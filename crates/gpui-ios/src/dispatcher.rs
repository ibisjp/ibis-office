// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/247)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
use crate::bridge;
use gpui::{PlatformDispatcher, Priority, RunnableVariant};
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::sync::{Arc, mpsc};
use std::thread::{self, ThreadId};
use std::time::Duration;

pub struct IosDispatcher {
    main_thread_id: ThreadId,
    main_queue: Arc<Mutex<VecDeque<RunnableVariant>>>,
    background_tx: mpsc::Sender<RunnableVariant>,
}

impl IosDispatcher {
    pub fn new() -> Self {
        let (background_tx, background_rx) = mpsc::channel::<RunnableVariant>();
        let rx = Arc::new(Mutex::new(background_rx));
        let worker_count = thread::available_parallelism()
            .map(|n| n.get().clamp(2, 4))
            .unwrap_or(2);
        for i in 0..worker_count {
            let rx = Arc::clone(&rx);
            let _ = thread::Builder::new()
                .name(format!("gpui-ios-bg-{i}"))
                .spawn(move || {
                    loop {
                        let task = {
                            let guard = rx.lock();
                            guard.recv()
                        };
                        match task {
                            Ok(runnable) => {
                                runnable.run();
                            }
                            Err(_) => break,
                        }
                    }
                });
        }

        Self {
            main_thread_id: thread::current().id(),
            main_queue: Arc::new(Mutex::new(VecDeque::new())),
            background_tx,
        }
    }

    pub fn drain_main_tasks(&self) {
        loop {
            let next = {
                let mut queue = self.main_queue.lock();
                queue.pop_front()
            };
            let Some(runnable) = next else {
                break;
            };
            runnable.run();
        }
    }
}

impl PlatformDispatcher for IosDispatcher {
    fn is_main_thread(&self) -> bool {
        thread::current().id() == self.main_thread_id
    }

    fn dispatch(&self, runnable: RunnableVariant, _priority: Priority) {
        if self.background_tx.send(runnable).is_err() {
            log::warn!("IosDispatcher background worker channel closed");
        }
    }

    fn dispatch_on_main_thread(&self, runnable: RunnableVariant, _priority: Priority) {
        self.main_queue.lock().push_back(runnable);
        bridge::wake_main();
    }

    fn dispatch_after(&self, duration: Duration, runnable: RunnableVariant) {
        let main_queue = Arc::clone(&self.main_queue);
        thread::spawn(move || {
            thread::sleep(duration);
            main_queue.lock().push_back(runnable);
            bridge::wake_main();
        });
    }

    fn spawn_realtime(&self, f: Box<dyn FnOnce() + Send>) {
        thread::spawn(f);
    }
}
