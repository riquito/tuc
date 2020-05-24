use anyhow::{Context, Result};
use std::io::Read;
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
    /// Fields to keep, e.g. 1-3 (both sides are optional)
    #[structopt(short, long, default_value="1-")]
    fields: String,
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