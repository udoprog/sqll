use sqll::{SendStatement, Statements};

#[derive(Statements)]
struct IntLiteral {
    #[sql(query = 42)]
    field: SendStatement,
}

fn main() {}
