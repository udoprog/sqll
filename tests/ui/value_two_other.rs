use sqll::FromColumn;

#[derive(FromColumn)]
#[sql(text)]
enum Kind {
    #[sql(other)]
    Unknown,
    File,
    #[sql(other)]
    Missing,
}

fn main() {}
