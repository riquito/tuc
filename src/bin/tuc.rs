use anyhow::{Context, Result};
use std::io::Read;
use std::str::FromStr;
use structopt::StructOpt;


#[derive(Debug, StructOpt)]
#[structopt(
    name = "tuc",
    about = "When cut doesn't cut it."
)]

struct Opt {
    /// Delimiter to use to cut the text into pieces
    #[structopt(short, long, default_value="\t")]
    delimiter: String,
    /// Fields to keep, like 1-3 or 3,2 or 1- or 3,1-2 or -3 or -3--2
    #[structopt(short, long, default_value="1-")]
    fields: RangeList,
}

#[derive(Debug)]
struct RangeList(Vec<Range>);

impl FromStr for RangeList {
    type Err = Box<dyn std::error::Error>;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(RangeList(vec![Range::default()]))
    }
}

#[derive(Debug)]
struct Range {
    l: usize,
    r: Option<usize>,
}

impl Range {
    pub fn new(l: usize, r: Option<usize>) -> Self {
        Range {
            l,
            r,
        }
    }
}

impl Default for Range {
    fn default() -> Self {
        Range::new(1, None)
    }
}


fn main() -> Result<()> {
    let opt = Opt::from_args();
    println!("{:?}",opt);

    let mut content = String::new();
    std::io::stdin()
            .read_to_string(&mut content)
            .with_context(|| format!("Cannot read from STDIN"));

    content = content.trim().to_string();

    let parts: Vec<&str> = content.split(&opt.delimiter).collect::<Vec<&str>>();
    println!("{:?}", parts);



    Ok(())
}