use bdecode::bdecode;

fn main() {
    let bytes = include_bytes!("../props/[ToishY] K-ON - THE COMPLETE SAGA (BD 1920x1080 x.264 FLAC).torrent");
    let torrent_file = bdecode(&bytes[..]).unwrap();
    let string = torrent_file.get_root().print_entry(true, 0);
    println!("{}", string);
}
