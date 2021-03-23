use std::fs::File;
use std::io::{BufWriter, Read, Seek, SeekFrom, Write};
use std::sync::Arc;

use crossbeam::channel;
use structopt::StructOpt;
use threadpool::ThreadPool;

mod cli;

use cli::Opt;

fn main() -> anyhow::Result<()> {
    let opt = Opt::from_args();
    let chunk = opt.chunk * 1024 * 1024;

    let pool = ThreadPool::new(opt.threads.cpus());
    let (worker_tx, worker_rx) = channel::unbounded();
    let size = ureq::get(&opt.url)
        .call()?
        .header("Content-Length")
        .map(|x| x.parse().unwrap());

    let mut writer = BufWriter::new(File::create(opt.output)?);

    if let Some(size) = size {
        let url = Arc::new(opt.url);

        for (worker_tx, x, y) in (0..size)
            .step_by(chunk)
            .map(|x| (x, if x + chunk > size { size } else { x + chunk } - 1))
            .map(|(x, y)| (worker_tx.clone(), x, y))
        {
            let url = url.clone();

            pool.execute(move || {
                let reader = ureq::get(url.as_ref())
                    .set("Range", format!("bytes={}-{}", x, y).as_ref())
                    .call()
                    .unwrap()
                    .into_reader();

                worker_tx.send((reader, x)).unwrap();
            });
        }

        drop(worker_tx);
        let mut buffer = [0u8; 8192];

        while let Ok((mut reader, x)) = worker_rx.recv() {
            writer.seek(SeekFrom::Start(x as u64))?;

            loop {
                let length = reader.read(&mut buffer)?;
                if length == 0 {
                    break;
                }

                writer.write_all(&buffer[..length])?;
            }
        }
    }

    Ok(())
}
