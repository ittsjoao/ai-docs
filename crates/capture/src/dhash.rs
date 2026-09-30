use image::imageops::{self, FilterType};
use image::RgbaImage;

/// Difference hash de 64 bits: o core compara prints pela distância de Hamming.
pub(crate) fn dhash(img: &RgbaImage) -> u64 {
    let gray = imageops::grayscale(img);
    let small = imageops::resize(&gray, 9, 8, FilterType::Triangle);
    let mut hash = 0u64;
    for y in 0..8 {
        for x in 0..8 {
            hash <<= 1;
            if small.get_pixel(x, y)[0] < small.get_pixel(x + 1, y)[0] {
                hash |= 1;
            }
        }
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    fn gradient(reverse: bool) -> RgbaImage {
        RgbaImage::from_fn(90, 80, |x, _| {
            let v = if reverse {
                255 - (x * 255 / 89)
            } else {
                x * 255 / 89
            } as u8;
            Rgba([v, v, v, 255])
        })
    }

    #[test]
    fn same_image_same_hash_and_opposite_gradients_far_apart() {
        assert_eq!(dhash(&gradient(false)), dhash(&gradient(false)));
        assert!((dhash(&gradient(false)) ^ dhash(&gradient(true))).count_ones() > 32);
    }

    #[test]
    fn tiny_change_stays_close() {
        let a = gradient(false);
        let mut b = a.clone();
        b.put_pixel(3, 3, Rgba([0, 0, 0, 255]));
        assert!((dhash(&a) ^ dhash(&b)).count_ones() <= 5);
    }
}
