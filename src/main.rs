use std::fs::File;
use std::io::{BufWriter, Seek, SeekFrom};
use std::sync::Arc;
use std::thread;

use crossbeam::channel;
use reqwest::{blocking, header};
use structopt::StructOpt;

mod cli;

use cli::Opt;

fn main() -> anyhow::Result<()> {
    let opt = Opt::from_args();
    let chunk = opt.chunk * 1024 * 1024;

    let (spawn_tx, spawn_rx) = channel::bounded(0);
    let (worker_tx, worker_rx) = channel::bounded(opt.threads.cpus());
    let size = blocking::get(&opt.url)?.content_length();

    let mut writer = BufWriter::new(File::create(opt.output)?);

    if let Some(size) = size {
        let size = size as usize;
        let url = Arc::new(opt.url);

        thread::spawn(move || {
            for (worker_tx, x, y) in (0..size)
                .step_by(chunk)
                .map(|x| (x, if x + chunk > size { size } else { x + chunk } - 1))
                .map(|(x, y)| (worker_tx.clone(), x, y))
            {
                if spawn_rx.recv().is_ok() {
                    let url = url.clone();

                    thread::spawn(move || {
                        let client = blocking::Client::new();
                        let res = client
                            .get(&*url)
                            .header(header::RANGE, format!("bytes={}-{}", x, y))
                            .send()
                            .unwrap();

                        let length = (y - x + 1) as u64;

                        worker_tx.send((res, x, length)).unwrap();
                    });
                } else {
                    break;
                }
            }
        });

        for _ in 0..opt.threads.cpus() {
            spawn_tx.send(());
        }

        while let Ok((mut res, x, mut length)) = worker_rx.recv() {
            writer.seek(SeekFrom::Start(x as u64))?;

            while length != 0 {
                length -= res.copy_to(&mut writer)?;
            }

            spawn_tx.send(());
        }
    }

    Ok(())
}

// fn spawn_workers(rx: channel::Receiver<()>) {
//     while rx.recv().is_ok() {
//         worker();
//     }
// }

// fn worker(url: impl AsRef<str>, mut x: usize, y: usize) {
//     let client = blocking::Client::new();
//     let mut res = client
//         .get(url)
//         .header(header::RANGE, format!("bytes={}-{}", x, y))
//         .send()
//         .await
//         .unwrap();

//     while let Some(bytes) = res.chunk().await.unwrap() {
//         let len = bytes.len();
//         tx.send((bytes, x)).await.unwrap();
//         x += len;
//     }
// }
