use std::path::PathBuf;
use std::{fmt, num, str};

use structopt::StructOpt;
use url::Url;

#[derive(Debug, StructOpt)]
pub struct Opt {
    /// The URL of the file to be downloaded
    #[structopt(long, short, parse(try_from_str))]
    pub(crate) url: Url,

    /// The output path where the file has to be saved
    #[structopt(long, short, parse(from_os_str))]
    pub(crate) output: Option<PathBuf>,

    /// Size of a download chunk in MB (defaults to 8MB)
    #[structopt(default_value = "8", long, short, parse(try_from_str))]
    pub(crate) chunk: usize,

    /// Number of threads (defaults to the number of logical cores)
    #[structopt(default_value, long, short, parse(try_from_str))]
    pub(crate) threads: CPUs,

    /// Use verbose output
    #[structopt(long, short, parse(from_occurrences))]
    pub(crate) verbose: u64,
}

impl Opt {
    pub fn verbose(&self) -> u64 {
        self.verbose
    }
}

#[derive(Debug)]
pub(crate) struct CPUs(pub(crate) usize);

impl Default for CPUs {
    fn default() -> Self {
        Self(num_cpus::get())
    }
}

impl fmt::Display for CPUs {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl str::FromStr for CPUs {
    type Err = num::ParseIntError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.parse()?))
    }
}
