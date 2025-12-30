use super::alloc;

#[test]
fn test_alloc_bytes() {
    let data = b"hello, world!";
    let (ptr, _, dealloc) = alloc(data).unwrap();

    if let Some(dealloc) = dealloc {
        unsafe {
            dealloc(ptr);
        }
    }
}

#[test]
fn test_alloc_bytes_empty() {
    let data = b"";
    let (ptr, _, dealloc) = alloc(data).unwrap();

    if let Some(dealloc) = dealloc {
        unsafe {
            dealloc(ptr);
        }
    }
}
