use bdecode::bdecode;
use std::collections::HashSet;

// TODO: This should rather be a function.
macro_rules! test_torrent_file {
    ($path:expr) => {
        let bytes = include_bytes!($path);

        let torrent = bdecode(&bytes[..]).unwrap();
        let top_level = torrent.get_root();

        let mut top_level_keys = HashSet::new();
        for i in 0 .. top_level.dict_size().unwrap() {
            let (key, value) = top_level.dict_at(i).unwrap();
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

        // TODO: This should test a bunch more variants, such as number of pieces (?), etc.
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
