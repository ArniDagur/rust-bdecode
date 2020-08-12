use url::Url;

use bdecode::bdecode;

use std::collections::HashSet;

fn test_torrent_file_bytes(bytes: &[u8]) {
    let torrent = bdecode(bytes).unwrap();
    let top_level = torrent.get_root().as_dict().unwrap();

    let mut top_level_keys = HashSet::new();
    for i in 0..top_level.len() {
        let (key, _value) = top_level.get(i).unwrap();
        top_level_keys.insert(String::from_utf8(key.to_vec()).unwrap());
    }
    assert!(top_level_keys.contains("announce"));
    assert!(top_level_keys.contains("announce-list"));
    assert!(top_level_keys.contains("comment"));
    assert!(top_level_keys.contains("created by"));
    assert!(top_level_keys.contains("creation date"));
    assert!(top_level_keys.contains("encoding"));
    assert!(top_level_keys.contains("info"));
    assert!(top_level_keys.len() == 7);

    // Check announce list-of-lists
    let announce_list = top_level.find(b"announce-list").unwrap().as_list().unwrap();
    for x in 0..announce_list.len() {
        let alternatives = announce_list.get(x).unwrap().as_list().unwrap();
        let num_alternatives = alternatives.len();
        assert!(num_alternatives > 0);
        for y in 0..num_alternatives {
            let alternative = alternatives.get(y).unwrap();
            // Make sure it's a valid URL with scheme UDP or HTTP
            let url_buf = alternative.as_string().unwrap().as_bytes();
            let url_string = String::from_utf8(url_buf.to_vec()).unwrap();
            let parsed_url = Url::parse(&url_string).unwrap();
            assert!((parsed_url.scheme() == "udp") || (parsed_url.scheme() == "http"));
        }
    }

    // Check that encoding is utf-8
    let encoding = top_level.find(b"encoding").unwrap().as_string().unwrap();
    assert_eq!(encoding.as_bytes(), b"utf-8");

    // Check that creation time is above the year 2000
    let creation_date = top_level.find(b"creation date").unwrap().as_int().unwrap();
    assert!(creation_date.value().unwrap() >= 946684800);
}

macro_rules! test_torrent_file {
    ($path:expr) => {
        let bytes = include_bytes!($path);
        test_torrent_file_bytes(&bytes[..]);
    };
}

#[test]
fn test_kon() {
    test_torrent_file!(
        "../props/[ToishY] K-ON - THE COMPLETE SAGA (BD 1920x1080 x.264 FLAC).torrent"
    );
}

#[test]
fn test_hibike_euphonium() {
    test_torrent_file!(
        "../props/[ToishY] Hibike! Euphonium - THE COMPLETE SAGA (BD 1920x1080 x264 FLAC).torrent"
    );
}

#[test]
fn test_touhou_lossless_collection() {
    test_torrent_file!("../props/Touhou lossless music collection.torrent");
}
