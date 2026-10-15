use sqll::{SendStatement, Statements};

#[derive(Statements)]
#[sql(nope)]
struct UnknownKey {
    #[sql = "SELECT 1"]
    field: SendStatement,
}

fn main() {}
