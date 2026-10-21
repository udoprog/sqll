use sqll::FromColumn;

#[derive(FromColumn)]
enum Direction {
    Input,
    Output,
}

fn main() {}
