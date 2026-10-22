use std::io::Cursor;

use gif::{ColorOutput, DecodeOptions, Encoder, Frame, Repeat};

const WIDTH: u16 = 2;
const HEIGHT: u16 = 1;
const PALETTE: &[u8] = &[255, 0, 0, 0, 0, 255];

#[test]
fn indexed_animation_encodes_repeat_timing_and_final_trailer() {
    let mut encoder = Encoder::new(Vec::new(), WIDTH, HEIGHT, PALETTE).unwrap();
    encoder.set_repeat(Repeat::Infinite).unwrap();

    let mut red = Frame::from_indexed_pixels(WIDTH, HEIGHT, vec![0, 0], None);
    red.delay = 5;
    encoder.write_frame(&red).unwrap();

    let mut blue = Frame::from_indexed_pixels(WIDTH, HEIGHT, vec![1, 1], None);
    blue.delay = 7;
    encoder.write_frame(&blue).unwrap();

    let bytes = encoder.into_inner().unwrap();
    assert_eq!(&bytes[..6], b"GIF89a");
    assert_eq!(bytes.last(), Some(&0x3b));

    let mut options = DecodeOptions::new();
    options.set_color_output(ColorOutput::RGBA);
    let mut decoder = options.read_info(Cursor::new(bytes)).unwrap();
    assert_eq!((decoder.width(), decoder.height()), (WIDTH, HEIGHT));
    assert_eq!(decoder.repeat(), Repeat::Infinite);

    let mut frames = Vec::new();
    while let Some(frame) = decoder.read_next_frame().unwrap() {
        frames.push((frame.delay, frame.buffer.to_vec()));
    }

    assert_eq!(frames.len(), 2);
    assert_eq!(frames[0], (5, vec![255, 0, 0, 255, 255, 0, 0, 255]));
    assert_eq!(frames[1], (7, vec![0, 0, 255, 255, 0, 0, 255, 255]));
}
