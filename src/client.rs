use std::cmp;
use std::fs::File;
use std::io::{self, Seek, SeekFrom, Write};
use std::sync::Arc;
use std::thread;

use crossbeam::channel;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use threadpool::ThreadPool;
use ureq::{Agent, AgentBuilder};
use url::Url;

use crate::Opt;
use crate::TrainError;

pub struct Client {
    agent: Agent,
    url: Arc<Url>,
    size: Option<usize>,
    chunk: usize,
    threads: usize,
    writer: File,
}

impl Client {
    pub fn new(opt: Opt) -> crate::Result<Self> {
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

    pub fn download(mut self) -> crate::Result<()> {
        if let Some(size) = self.size {
            let multi_progress_bar = Arc::new(MultiProgress::new());
            let progress_bar_style = ProgressStyle::default_bar()
                .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")
                .progress_chars("#>-");

            let pool = ThreadPool::new(self.threads);
            let (worker_tx, worker_rx) = channel::unbounded();
            let (progress_tx, progress_rx) = channel::bounded(self.threads);

            (0..=self.threads)
                .skip(1)
                .for_each(|x| progress_tx.send(x).unwrap());

            for x in (0..size).step_by(self.chunk) {
                let worker_tx = worker_tx.clone();
                let agent = self.agent.clone();
                let url = self.url.clone();
                let y = cmp::min(x + self.chunk, size) - 1;
                let capacity = y - x + 1;

                let multi_progress_bar = multi_progress_bar.clone();
                let progress_bar_style = progress_bar_style.clone();
                let progress_tx = progress_tx.clone();
                let progress_rx = progress_rx.clone();

                pool.execute(move || {
                    let index = progress_rx.recv().unwrap();
                    let progress_bar = multi_progress_bar
                        .insert(index, ProgressBar::new(capacity as u64))
                        .with_style(progress_bar_style);

                    let downloader = Downloader { agent, url, x, y };
                    let message = download_range(downloader, progress_bar.clone());

                    worker_tx.send(message).unwrap();
                    progress_bar.finish_and_clear();
                    progress_tx.send(index).unwrap();
                });
            }

            drop(worker_tx);
            let progress_bar = multi_progress_bar
                .insert(0, ProgressBar::new(size as u64))
                .with_style(progress_bar_style);
            let mut writer = DownloadProgressBar {
                bar: progress_bar.clone(),
                buffer: self.writer,
            };

            let main_thread = thread::spawn(move || -> crate::Result<()> {
                writer.bar.set_position(0);

                while let Ok(message) = worker_rx.recv() {
                    let DownloadPart { x, buf } = message?;
                    writer
                        .seek(SeekFrom::Start(x as u64))
                        .map_err(TrainError::FileSeekError)?;
                    writer.write_all(&buf).map_err(TrainError::FileWriteError)?;
                }

                progress_bar.finish();
                writer
                    .buffer
                    .sync_all()
                    .map_err(TrainError::FileSyncError)?;

                Ok(())
            });

            multi_progress_bar.join().unwrap();
            main_thread.join().unwrap()?;
        } else {
            let mut reader = self
                .agent
                .get(self.url.as_ref().as_ref())
                .call()?
                .into_reader();
            io::copy(&mut reader, &mut self.writer).map_err(TrainError::FileWriteError)?;
            self.writer.sync_all().map_err(TrainError::FileSyncError)?;
        }

        Ok(())
    }
}

fn download_range(
    downloader: Downloader,
    progress_bar: ProgressBar,
) -> crate::Result<DownloadPart> {
    let Downloader { agent, url, x, y } = downloader;

    let capacity = y - x + 1;
    let mut buf = Vec::with_capacity(capacity);

    let mut reader = agent
        .get(url.as_ref().as_ref())
        .set("Range", format!("bytes={}-{}", x, y).as_ref())
        .call()?
        .into_reader();

    io::copy(&mut reader, &mut progress_bar.wrap_write(&mut buf))
        .map_err(TrainError::BufferWriteError)?;
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

struct DownloadProgressBar<B> {
    bar: ProgressBar,
    buffer: B,
}

impl<R: io::Read> io::Read for DownloadProgressBar<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let inc = self.buffer.read(buf)?;
        self.bar.inc(inc as u64);
        Ok(inc)
    }
}

impl<S: io::Seek> io::Seek for DownloadProgressBar<S> {
    fn seek(&mut self, f: io::SeekFrom) -> io::Result<u64> {
        self.buffer.seek(f)
    }
}

impl<W: io::Write> io::Write for DownloadProgressBar<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.buffer.write(buf).map(|inc| {
            self.bar.inc(inc as u64);
            inc
        })
    }

    fn flush(&mut self) -> io::Result<()> {
        self.buffer.flush()
    }

    fn write_vectored(&mut self, bufs: &[io::IoSlice]) -> io::Result<usize> {
        self.buffer.write_vectored(bufs).map(|inc| {
            self.bar.inc(inc as u64);
            inc
        })
    }

    fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        self.buffer.write_all(buf).map(|()| {
            self.bar.inc(buf.len() as u64);
        })
    }
}
