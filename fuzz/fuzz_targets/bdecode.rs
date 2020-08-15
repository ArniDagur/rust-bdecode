#![no_main]
use bdecode::{bdecode, NodeType};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(bencode) = bdecode(&data) {
        let root_node = bencode.get_root();
        match root_node.node_type() {
            NodeType::Dict => {
                let dict = root_node.as_dict().unwrap();
                dict.len();
                // These return Result<T> may either succeed or fail, but not
                // panic.
                dict.get(0);
                dict.get(0);
                dict.find(b"abc");
            }
            NodeType::List => {
                let list = root_node.as_list().unwrap();
                list.len();
                list.get(0);
                list.get(1);
            }
            NodeType::Int => {
                let int = root_node.as_int().unwrap();
                int.value();
            }
            NodeType::Str => {
                let string = root_node.as_string().unwrap();
                string.as_bytes();
            }
            _ => unreachable!(),
        }
    };
});
