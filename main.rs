use std::env;
use std::io::{self, Write};
mod taskmaster;

fn main() {

    let args: Vec<String> = env::args().collect();
    let equation = if args.len() == 2 {
        args[1].clone()
    }
    else {
        println!("supervisor>");
        io::stdout().flush().unwrap();
        let mut input = String::new();
        io::stdin().read_line(&mut input).expect("Failed to read line");
        input.trim().to_string()
    };
    taskmaster(&equation);
}
