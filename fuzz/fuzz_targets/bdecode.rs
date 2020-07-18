#![no_main]
use bdecode::{bdecode, NodeType};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(bencode) = bdecode(&data) {
        let root_node = bencode.get_root();
        match root_node.node_type() {
            NodeType::Dict => {
                root_node.dict_size().unwrap();
                root_node.dict_at(0);
                root_node.dict_at(1);
                root_node.dict_find(b"abc");
            }
            NodeType::List => {
                root_node.list_size().unwrap();
                root_node.list_at(0);
                root_node.list_at(1);
            }
            NodeType::Int => {
                root_node.int_value();
            }
            NodeType::Str => {
                root_node.string_buf();
            }
            _ => unreachable!(),
        }
    };
});
