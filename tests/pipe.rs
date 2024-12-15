use encrypted_pipe::prelude::*;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    time::timeout,
};

#[tokio::test]
async fn pipe_split() {
    // making a simplex stream, just for testing
    let (reader, writer) = tokio::io::simplex(64);
    let stream = reader.unsplit(writer);

    // reader <-> writer

    let (mut reader, mut writer) = split(stream, None);
    let mut reader2 = writer.reader(None);

    // reader <-> writer -> reader2
    // reader and writer initially came from the same stream, reader2 reads from writer as well as reader

    let data = b"test data".to_vec();
    println!("data: {:?}", std::str::from_utf8(&data));
    // writer.write_all(&data).await.unwrap();
    writer.write_all(&data).await.unwrap();

    let mut buf = Vec::new();
    // reader.read_buf(&mut buf).await.unwrap();
    reader.read_buf(&mut buf).await.unwrap();

    let mut buf2 = Vec::new();
    reader2.read_buf(&mut buf2).await.unwrap();

    println!("buf: {:?}", std::str::from_utf8(&buf));
    println!("buf2: {:?}", std::str::from_utf8(&buf2));

    assert_eq!(buf, data);
    assert_eq!(buf2, data);

    // reader <-> writer -> reader2 <- writer2 -> reader3

    // both writer and writer2 write into reader2 yet writer2 does not write to reader. only writer2 writes into reader3

    let mut writer2 = reader2.writer(None);
    let mut reader3 = writer2.reader(None);
    let data = b"test data 2".to_vec();

    writer2.write_all(&data).await.unwrap();

    let mut buf = Vec::new();
    // had to write a timeout because this would hang forever trying to read from stuff
    let _ = timeout(
        tokio::time::Duration::from_millis(500),
        reader.read_buf(&mut buf),
    )
    .await;

    let mut buf2 = Vec::new();
    reader2.read_buf(&mut buf2).await.unwrap();

    let mut buf3 = Vec::new();
    reader3.read_buf(&mut buf3).await.unwrap();

    println!("buf: {:?}", std::str::from_utf8(&buf)); // empty (hanged forever, waiting for writer)
    println!("buf2: {:?}", std::str::from_utf8(&buf2)); // copied data
    println!("buf3: {:?}", std::str::from_utf8(&buf3)); // copied data

    assert_eq!(buf, []);
    assert_eq!(buf2, data);
    assert_eq!(buf3, data);

    // reader <-> writer -> reader2 <- writer2 | reader3

    // detach reader3 from writer2
    writer2.detach(&reader3);

    writer2.write_all(&data).await.unwrap();

    let mut buf = Vec::new();
    // had to write a timeout because this would hang forever trying to read from stuff
    let _ = timeout(
        tokio::time::Duration::from_millis(500),
        reader3.read_buf(&mut buf),
    )
    .await;

    let mut buf2 = Vec::new();
    reader2.read_buf(&mut buf2).await.unwrap();

    println!("buf: {:?}", std::str::from_utf8(&buf)); // empty (hanged forever, waiting for writer)
    println!("buf2: {:?}", std::str::from_utf8(&buf2)); // copied data

    assert_eq!(buf, []);
    assert_eq!(buf2, data);

    // reattach reader3 to writer2
    writer2.attach(&mut reader3);

    writer2.write_all(&data).await.unwrap();

    let mut buf = Vec::new();
    reader3.read_buf(&mut buf).await.unwrap();
    let mut buf2 = Vec::new();
    reader2.read_buf(&mut buf2).await.unwrap();

    println!("buf: {:?}", std::str::from_utf8(&buf)); // should be reattached, copied data
    println!("buf2: {:?}", std::str::from_utf8(&buf2)); // copied data

    assert_eq!(buf, data);
    assert_eq!(buf2, data);

    //? turn on to see prints and debug
    assert!(false);
}
