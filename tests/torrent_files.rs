use url::Url;

use bdecode::bdecode;

use std::collections::HashSet;

// TODO: This should rather be a function.
macro_rules! test_torrent_file {
    ($path:expr) => {
        let bytes = include_bytes!($path);

        let torrent = bdecode(&bytes[..]).unwrap();
        let top_level = torrent.get_root();

        let mut top_level_keys = HashSet::new();
        for i in 0..top_level.dict_size().unwrap() {
            let (key, _value) = top_level.dict_at(i).unwrap();
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
        let announce_list = top_level.dict_find(b"announce-list").unwrap().unwrap();
        for x in 0..announce_list.list_size().unwrap() {
            let alternatives = announce_list.list_at(x).unwrap();
            let num_alternatives = alternatives.list_size().unwrap();
            assert!(num_alternatives > 0);
            for y in 0..num_alternatives {
                let alternative = alternatives.list_at(y).unwrap();
                // Make sure it's a valid URL with scheme UDP or HTTP
                let url_buf = alternative.string_buf().unwrap();
                let url_string = String::from_utf8(url_buf.to_vec()).unwrap();
                let parsed_url = Url::parse(&url_string).unwrap();
                assert!((parsed_url.scheme() == "udp") || (parsed_url.scheme() == "http"));
            }
        }

        // Check that encoding is utf-8
        let encoding = top_level.dict_find(b"encoding").unwrap().unwrap();
        assert_eq!(encoding.string_buf().unwrap(), b"utf-8");

        // Check that creation time is above the year 2000
        let creation_date = top_level.dict_find(b"creation date").unwrap().unwrap();
        assert!(creation_date.int_value().unwrap() >= 946684800);
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
