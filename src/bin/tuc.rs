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


fn main() {
    let opt = Opt::from_args();
    println!("{:?}",opt);
}