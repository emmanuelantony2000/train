use std::fs::File;
use std::io::{BufWriter, Seek, SeekFrom, Write};
use std::sync::Arc;

use anyhow::{anyhow, bail};
use reqwest::{header, Client};
use tokio::sync::{mpsc, Notify};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // let url = "https://desktop.docker.com/mac/stable/Docker.dmg";
    // let url = "https://filesamples.com/samples/document/txt/sample1.txt";
    let url = "https://mirrors.edge.kernel.org/archlinux/iso/2021.03.01/archlinux-bootstrap-2021.03.01-x86_64.tar.gz";
    let chunk = 8 * 1024 * 1024;

    // let cpus = num_cpus::get();
    let cpus = 8;

    let notify = Arc::new(Notify::new());
    let (tx, mut rx) = mpsc::channel(cpus);

    let client = Client::new();
    let response = client.get(url).send().await?;

    let size = response.content_length();

    // let filename = response
    //     .headers()
    //     .get(header::CONTENT_DISPOSITION)
    //     .ok_or(anyhow!("Error while accessing header"))?
    //     .to_str()?;
    // let filename = if filename.starts_with("attachment") {
    //     let filename = filename.trim_start_matches("attachment; filename=");
    //     filename.trim_matches('\"')
    // } else {
    //     bail!("Not a downloadable file");
    // };

    let file = File::create("arch.tar.gz")?;
    let mut writer = BufWriter::new(file);

    if let Some(size) = size {
        let size = size as usize;

        for (notify, tx, mut x, y) in (0..size)
            .step_by(chunk)
            .map(|x| (x, if x + chunk > size { size } else { x + chunk } - 1))
            .map(|(x, y)| (notify.clone(), tx.clone(), x, y))
        {
            tokio::spawn(async move {
                let client = Client::new();
                notify.notified().await;
                let mut res = client
                    .get(url)
                    .header(header::RANGE, format!("bytes={}-{}", x, y))
                    .send()
                    .await
                    .unwrap();

                while let Some(bytes) = res.chunk().await.unwrap() {
                    let len = bytes.len();
                    tx.send((bytes, x)).await.unwrap();
                    x += len;
                }
            });
        }

        drop(tx);

        for _ in 0..cpus {
            notify.notify_one();
        }

        while let Some((bytes, x)) = rx.recv().await {
            writer.seek(SeekFrom::Start(x as u64))?;
            writer.write_all(&bytes)?;

            notify.notify_one();
        }
    }

    Ok(())
}
