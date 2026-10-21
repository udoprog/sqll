use sqll::{SendStatement, Statements};

const NOT_STR: u32 = 1;

#[derive(Statements)]
struct NotStr {
    #[sql(query = NOT_STR)]
    field: SendStatement,
}

fn main() {}
