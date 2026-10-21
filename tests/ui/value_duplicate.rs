use sqll::FromColumn;

#[derive(FromColumn)]
#[sql(integer)]
enum Field {
    Name = 1,
    Age = 2,
    #[sql(value = 1)]
    Email,
}

fn main() {}
