use encrypted_pipe::prelude::*;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    time::timeout,
};

#[tokio::test]
async fn basic() {
    // Chain visualization:
    // A <- B -> C <- D

    // where B and D are writers
    // and A and C are readers

    // assume we already have a flow of data,
    // this is A <- B yet neither A or B are wrapped yet
    let (reader, writer) = tokio::io::simplex(4096);

    let mut b = MultiWriter::new(writer, None, Some(4096)); // manual writer wrapping
    let mut a = MultiReader::new(reader, None, Some(4096)); // manual reader wrapping
    let mut c = b.reader(None).unwrap(); // auto reader creation, pre-attached to B
    let mut d = c.writer(None).unwrap(); // auto-writer creation, pre-attached to C

    // testing the chain
    let message_from_b = b"Message from B";
    let message_from_d = b"Message from D";

    // allow 500ms for each reader to run it's recv
    let max_time = tokio::time::Duration::from_millis(500);

    b.write_all(message_from_b).await.unwrap();
    d.write_all(message_from_d).await.unwrap();

    // should work fine
    let mut a_read_1 = Vec::new();
    timeout(max_time, a.read_buf(&mut a_read_1))
        .await
        .unwrap()
        .unwrap();

    // timeout because A doesn't get D's messages
    let mut a_read_2 = Vec::new();
    timeout(max_time, a.read_buf(&mut a_read_2))
        .await
        .unwrap_err(); // expect a error

    // C should receive from both B and D
    let mut c_read_1 = Vec::new();
    timeout(max_time, c.read_buf(&mut c_read_1))
        .await
        .unwrap()
        .unwrap();

    let mut c_read_2 = Vec::new();
    timeout(max_time, c.read_buf(&mut c_read_2))
        .await
        .unwrap()
        .unwrap();

    assert_eq!(a_read_1, message_from_b);
    assert!(a_read_2.is_empty());

    assert_eq!(c_read_1, message_from_b);
    assert_eq!(c_read_2, message_from_d);
}

#[tokio::test]
async fn detach() {
    // where B and D are writers
    // and A and C are readers

    // assume we already have a flow of data,
    // this is A <- B yet neither A or B are wrapped yet
    let (a, b) = tokio::io::simplex(4096);
    let (c, d) = tokio::io::simplex(4096);

    let mut b = MultiWriter::new(b, None, Some(4096)); // writer
    let mut a = MultiReader::new(a, None, Some(4096)); // reader
    let mut c = MultiReader::new(c, None, Some(4096)); // reader
    let mut d = MultiWriter::new(d, None, Some(4096)); // writer

    // Current state of the chain:
    // A <- B | C <- D
    // (| means no connection)

    // Attaching B to C
    c.attach(&mut b).unwrap();
    //? This can also be done like so:
    // b.attach(&mut c).unwrap();

    // Current state of the chain:
    // A <- B -> C <- D

    let message_to_a_and_c = b"Message to A and C";
    let message_to_a_only = b"Message to A only";
    let message_to_c_only = b"Message to C only";

    // allow 500ms for each reader to run it's recv
    let max_time = tokio::time::Duration::from_millis(500);

    b.write_all(message_to_a_and_c).await.unwrap();

    let mut a_read_1 = Vec::new();
    timeout(max_time, a.read_buf(&mut a_read_1))
        .await
        .unwrap()
        .unwrap();

    let mut c_read_1 = Vec::new();
    timeout(max_time, c.read_buf(&mut c_read_1))
        .await
        .unwrap()
        .unwrap();

    //? You can detach a writer from a reader like so:
    b.detach(&c).unwrap();
    //? Or you can do the opposite like so:
    // c.detach(&b).unwrap();

    //* You can't detach a writer from a reader that were already connected at spawn
    //* that means, if the reader was spawned from that writer or the writer spawned
    //* from that reader (using the .reader() or .writer() methods) then you can't
    //* detach it, ever.
    // The following will never work and always panic.
    // That is because
    // B is externally attached to A
    // and
    // D is externally attached to C
    a.detach(&b).unwrap_err();
    b.detach(&a).unwrap_err(); // also works both ways

    // Write out our messages
    b.write_all(message_to_a_only).await.unwrap();
    d.write_all(message_to_c_only).await.unwrap();

    let mut a_read_2 = Vec::new();
    timeout(max_time, a.read_buf(&mut a_read_2))
        .await
        .unwrap()
        .unwrap();

    // timeout because C should not receive from B anymore
    let mut c_read_2 = Vec::new();
    timeout(max_time, c.read_buf(&mut c_read_2))
        .await
        .unwrap()
        .unwrap();

    assert_eq!(a_read_1, message_to_a_and_c);
    assert_eq!(a_read_2, message_to_a_only);
    assert_eq!(c_read_1, message_to_a_and_c);
    assert_eq!(c_read_2, message_to_c_only);
}
