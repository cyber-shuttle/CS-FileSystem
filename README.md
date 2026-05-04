# CS-FileSystem
User Space File System to populate Cybershuttle Data Sources


sudo apt install cargo

sudo apt install -y libfuse3-dev libfuse-dev pkg-config

cargo build

mkdir /tmp/myfs
cargo run --release -- /tmp/myfs


In a different terminal
ls /tmp/myfs


To unmount
fusermount -u /tmp/myfs
