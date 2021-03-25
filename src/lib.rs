use std::io::{self, Seek, SeekFrom};
use std::sync::Arc;
use std::{fs::File, io::Write};

use crossbeam::channel;
use threadpool::ThreadPool;
use ureq::{Agent, AgentBuilder};
use url::Url;

mod cli;
mod error;

pub use cli::Opt;
use error::TrainError;

pub struct Client {
    agent: Agent,
    url: Arc<Url>,
    size: Option<usize>,
    chunk: usize,
    threads: usize,
    writer: File,
}

impl Client {
    pub fn new(opt: Opt) -> error::Result<Self> {
        let chunk = opt.chunk * 1024 * 1024;
        let threads = opt.threads.0;
        let agent = AgentBuilder::new()
            .max_idle_connections_per_host(100)
            .build();

        let response = agent.get(opt.url.as_ref()).call()?;
        let size = response
            .header("Content-Length")
            .and_then(|x| x.parse().ok());
        let url: Arc<Url> = Arc::new(response.get_url().parse()?);

        let filename = if let Some(output) = opt.output {
            output
        } else {
            url.as_ref()
                .path_segments()
                .and_then(|segments| segments.last())
                .and_then(|name| if !name.is_empty() { Some(name) } else { None })
                .ok_or(TrainError::FilenameParseError)?
                .parse()
                .unwrap()
        };
        let writer = File::create(filename).map_err(TrainError::FileCreateError)?;

        Ok(Self {
            agent,
            url,
            size,
            chunk,
            threads,
            writer,
        })
    }

    pub fn download(&mut self) -> error::Result<()> {
        if let Some(size) = self.size {
            let pool = ThreadPool::new(self.threads);
            let (worker_tx, worker_rx) = channel::unbounded();

            for (worker_tx, agent, url, x, y) in (0..size).step_by(self.chunk).map(|x| {
                let worker_tx = worker_tx.clone();
                let agent = self.agent.clone();
                let url = self.url.clone();

                let y = if x + self.chunk > size {
                    size
                } else {
                    x + self.chunk
                } - 1;

                (worker_tx, agent, url, x, y)
            }) {
                pool.execute(move || {
                    let downloader = Downloader { agent, url, x, y };
                    let message = download_range(downloader);
                    worker_tx.send(message).unwrap();
                });
            }

            drop(worker_tx);

            while let Ok(message) = worker_rx.recv() {
                let DownloadPart { x, buf } = message?;
                self.writer
                    .seek(SeekFrom::Start(x as u64))
                    .map_err(TrainError::FileSeekError)?;
                self.writer
                    .write_all(&buf)
                    .map_err(TrainError::FileWriteError)?;
            }
        }

        self.writer.sync_all().map_err(TrainError::FileSyncError)?;

        Ok(())
    }
}

fn download_range(downloader: Downloader) -> error::Result<DownloadPart> {
    let Downloader { agent, url, x, y } = downloader;

    let capacity = y - x + 1;
    let mut buf = Vec::with_capacity(capacity);

    let mut reader = agent
        .get(url.as_ref().as_ref())
        .set("Range", format!("bytes={}-{}", x, y).as_ref())
        .call()?
        .into_reader();

    io::copy(&mut reader, &mut buf).map_err(TrainError::FileWriteError)?;
    let part = DownloadPart { x, buf };

    Ok(part)
}

struct Downloader {
    agent: Agent,
    url: Arc<Url>,
    x: usize,
    y: usize,
}

struct DownloadPart {
    x: usize,
    buf: Vec<u8>,
}
