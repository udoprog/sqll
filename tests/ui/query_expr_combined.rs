use sqll::{SendStatement, Statements};

const SELECT: &str = "SELECT 1";

#[derive(Statements)]
struct Combined {
    #[sql = "SELECT"]
    #[sql(query = SELECT)]
    field: SendStatement,
}

fn main() {}
