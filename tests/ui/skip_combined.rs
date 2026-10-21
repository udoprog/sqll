use sqll::{SendStatement, Statements};

#[derive(Statements)]
struct SkipCombined {
    #[sql(skip, statements)]
    nested: SendStatement,
    #[sql(skip)]
    #[sql = "SELECT 1"]
    query: SendStatement,
}

fn main() {}
