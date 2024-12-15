use encrypted_pipe::prelude::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::test]
async fn test_crypto() {
    let key = [0u8; 32];
    let (reader, writer) = encrypted_duplex(&key);
    // wrapping our reader and writer in our pipes
    let mut reader = PipeReader::new(reader, None, None);
    let mut writer = PipeWriter::new(writer, None, None);

    // test prime conditions
    let data = b"test data".to_vec();

    writer.write_all(&data).await.unwrap();
    writer.flush().await.unwrap();

    let mut buf = Vec::new();
    reader.read_buf(&mut buf).await.unwrap();

    println!("Prime test");
    println!("buf: {:?}", std::str::from_utf8(&buf));
    println!("data: {:?}", std::str::from_utf8(&data));

    assert_eq!(buf, data);

    // test writing again, making sure cleanup was done after our last one
    let data = b"test data 2".to_vec();
    writer.write_all(&data).await.unwrap();
    writer.flush().await.unwrap();

    let mut buf = Vec::new();
    reader.read_buf(&mut buf).await.unwrap();

    println!("Writing test");
    println!("buf: {:?}", std::str::from_utf8(&buf));
    println!("data: {:?}", std::str::from_utf8(&data));

    assert_eq!(buf, data);

    // test writing, then writing more, then reading a bit and then reading all
    let data_part_1 = b"test ".to_vec();
    let data_part_2 = b"data ".to_vec();
    let data_part_3 = b"again".to_vec();
    let mut data_concat: Vec<u8> = Vec::new();
    data_concat.extend(&data_part_1);
    data_concat.extend(&data_part_2);
    data_concat.extend(&data_part_3);

    writer.write_all(&data_part_1).await.unwrap();
    writer.write_all(&data_part_2).await.unwrap();
    writer.write_all(&data_part_3).await.unwrap();
    writer.flush().await.unwrap();

    let mut buf = Vec::new();
    reader.read_buf(&mut buf).await.unwrap();

    println!("Concat test");
    println!("buf: {:?}", std::str::from_utf8(&buf));
    println!("data: {:?}", std::str::from_utf8(&data_concat));

    assert_eq!(buf, data_concat);

    // now we can hook up our reader and writers to other stuff
    let mut reader_copy = writer.reader(None);
    let data = b"integration test data".to_vec();

    writer.write_all(&data).await.unwrap();
    writer.flush().await.unwrap();

    let mut buf = Vec::new();
    reader_copy.read_buf(&mut buf).await.unwrap();
    let mut buf2 = Vec::new();
    reader.read_buf(&mut buf2).await.unwrap();

    println!("Integration test");
    println!("data: {:?}", std::str::from_utf8(&data));
    println!("buf: {:?}", std::str::from_utf8(&buf));
    println!("buf2: {:?}", std::str::from_utf8(&buf2));

    assert_eq!(buf, data);
    assert_eq!(buf2, data);

    //? turn on to see prints and debug
    assert!(false);
}
