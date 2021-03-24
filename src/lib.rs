use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::sync::Arc;

use crossbeam::channel;
use threadpool::ThreadPool;
use ureq::Agent;
use url::Url;

mod cli;
mod error;

pub use cli::Opt;
use error::TrainError;

pub struct Client {
    pub agent: Agent,
    pub url: Arc<Url>,
    pub size: Option<usize>,
    pub chunk: usize,
    pub threads: usize,
    pub writer: File,
}

impl Client {
    pub fn new(opt: Opt) -> error::Result<Self> {
        let chunk = opt.chunk * 1024 * 1024;
        let threads = opt.threads.0;
        let agent = Agent::new();

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
                (
                    worker_tx.clone(),
                    self.agent.clone(),
                    self.url.clone(),
                    x,
                    if x + self.chunk > size {
                        size
                    } else {
                        x + self.chunk
                    } - 1,
                )
            }) {
                pool.execute(move || {
                    let message = download_range(agent, url, x, y);
                    worker_tx.send(message).unwrap();
                });
            }

            drop(worker_tx);

            while let Ok(message) = worker_rx.recv() {
                let (mut reader, x) = message?;
                self.writer
                    .seek(SeekFrom::Start(x as u64))
                    .map_err(TrainError::FileSeekError)?;
                io::copy(&mut reader, &mut self.writer).map_err(TrainError::FileWriteError)?;
            }
        }

        self.writer.sync_all().map_err(TrainError::FileSyncError)?;

        Ok(())
    }
}

fn download_range(
    agent: Agent,
    url: Arc<Url>,
    x: usize,
    y: usize,
) -> error::Result<(impl Read + Send, usize)> {
    Ok((
        agent
            .get(url.as_ref().as_ref())
            .set("Range", format!("bytes={}-{}", x, y).as_ref())
            .call()?
            .into_reader(),
        x,
    ))
}
