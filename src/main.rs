use clap::Parser;

#[derive(Debug, clap::Parser)]
struct Args {}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    println!("Hello, world! {:?}", args);
}
