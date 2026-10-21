use sqll::FromColumn;

const BASE: isize = 10;

#[derive(FromColumn)]
#[sql(integer)]
enum Level {
    Low = BASE,
    High,
}

fn main() {}
