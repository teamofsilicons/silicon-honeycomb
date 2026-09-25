//! Human progress goes to stderr; JSON and unattended maintenance remain silent.
use honeycomb_client::progress::ProgressEvent;
use std::{
    io::{IsTerminal, Write},
    sync::{Mutex, mpsc},
    thread,
    time::{Duration, Instant},
};

enum Message {
    Stage(String),
    Pause(mpsc::Sender<()>),
}

pub struct Progress {
    enabled: bool,
    terminal: bool,
    sender: Option<mpsc::Sender<Message>>,
    worker: Option<thread::JoinHandle<()>>,
    last_download: Mutex<Option<Instant>>,
    completion: Mutex<Option<String>>,
}
impl Progress {
    pub fn new(enabled: bool, message: &str) -> Self {
        let terminal = enabled && std::io::stderr().is_terminal();
        let mut progress = Self {
            enabled,
            terminal,
            sender: None,
            worker: None,
            last_download: Mutex::new(None),
            completion: Mutex::new(None),
        };
        if enabled {
            eprintln!("{message}…");
        }
        if terminal {
            let (sender, receiver) = mpsc::channel::<Message>();
            let mut message = message.to_owned();
            progress.sender = Some(sender);
            progress.worker = Some(thread::spawn(move || {
                let mut frame = 0;
                let mut paused = false;
                loop {
                    match receiver.recv_timeout(Duration::from_millis(100)) {
                        Ok(Message::Stage(next)) => {
                            message = next;
                            paused = false;
                        }
                        Ok(Message::Pause(done)) => {
                            let mut stderr = std::io::stderr().lock();
                            let _ = write!(stderr, "\r\x1b[2K");
                            let _ = stderr.flush();
                            paused = true;
                            let _ = done.send(());
                            continue;
                        }
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                    }
                    if paused {
                        continue;
                    }
                    let mut stderr = std::io::stderr().lock();
                    let _ = write!(
                        stderr,
                        "\r\x1b[2K{} {message}",
                        ['|', '/', '-', '\\'][frame % 4]
                    );
                    let _ = stderr.flush();
                    frame += 1;
                }
                let _ = write!(std::io::stderr(), "\r\x1b[2K");
            }));
        }
        progress
    }
    /// True when a person is watching and can answer a question.
    pub fn interactive(&self) -> bool {
        self.terminal
    }
    pub fn stage(&self, message: &str) {
        if !self.enabled {
            return;
        }
        if let Some(sender) = &self.sender {
            let _ = sender.send(Message::Stage(message.to_owned()));
        } else {
            eprintln!("{message}…");
        }
    }
    /// Clear and pause the spinner before the caller prints a command result.
    pub fn pause(&self) {
        if let Some(sender) = &self.sender {
            let (done, received) = mpsc::channel();
            if sender.send(Message::Pause(done)).is_ok() {
                let _ = received.recv();
            }
        }
    }
    pub fn event(&self, event: ProgressEvent) {
        if !self.enabled {
            return;
        }
        match event {
            ProgressEvent::Stage(message) => self.stage(message),
            ProgressEvent::Download { received, total } => {
                let mut last = self.last_download.lock().unwrap();
                let interval = if self.terminal {
                    Duration::from_millis(100)
                } else {
                    Duration::from_secs(1)
                };
                if received != 0
                    && Some(received) != total
                    && last.is_some_and(|time| time.elapsed() < interval)
                {
                    return;
                }
                *last = Some(Instant::now());
                let amount = received as f64 / 1_048_576.0;
                let message = match total.filter(|total| *total > 0) {
                    Some(total) => format!(
                        "Downloading: {:.0}% ({amount:.1} / {:.1} MiB)",
                        (received as f64 / total as f64 * 100.0).min(100.0),
                        total as f64 / 1_048_576.0
                    ),
                    None => format!("Downloading: {amount:.1} MiB received"),
                };
                self.stage(&message);
            }
        }
    }
    pub fn completion(&self, message: String) {
        *self.completion.lock().unwrap() = Some(message);
    }
    pub fn finish(mut self, success: bool) {
        self.stop();
        if self.enabled {
            if success {
                eprintln!(
                    "{}",
                    self.completion
                        .lock()
                        .unwrap()
                        .as_deref()
                        .unwrap_or("Done.")
                );
            } else {
                // A completion recorded before the failure means the installation itself
                // finished; only a later step such as the package's install script failed.
                eprintln!(
                    "{}",
                    self.completion
                        .lock()
                        .unwrap()
                        .as_deref()
                        .unwrap_or("Installation/update failed.")
                );
            }
        }
    }
    fn stop(&mut self) {
        self.sender.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
impl Drop for Progress {
    fn drop(&mut self) {
        self.stop();
    }
}
