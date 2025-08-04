use image::{ImageBuffer, Rgba, RgbaImage, load_from_memory};
use kmeans_colors::get_kmeans;
use palette::{IntoColor, Lab, Srgb, cast::from_component_slice};
use reqwest;

use std::time::Instant;

/// The different options to create an gradient overly
/// Dominant: search for the most dominat color in the whole image
/// DominantBottom: search for the most dominat color in the bottom row of the image
/// UserSelected: use the given rgb color as the overlay
pub enum GradientColorType {
    Dominant,
    DominantBottom,
    UserSelected(u8, u8, u8),
}
///
/// Fetch an imge from the given url and create a new image with the specified overlay
/// and save the image to disk
/// The overlay is constucted to got from the botom to 60% of the image hight where it will be no
/// overlay and up to the top increasing the overlay color
pub async fn generate_from_url(
    url: String,
    gradient_variant: GradientColorType,
    fade: f32,
) -> ImageBuffer<image::Rgba<u8>, Vec<u8>> {
    let start = Instant::now();
    let response = reqwest::get(url).await.expect("Failed to fetch image");
    let duration = start.elapsed();
    println!("Request took: {:?}", duration);
    let start = Instant::now();
    let bytes = response.bytes().await.expect("Failed to load image data");
    let duration = start.elapsed();
    println!("get bytes took: {:?}", duration);
    let start = Instant::now();
    let dynamic_img = load_from_memory(&bytes).expect("Failed to load image from memory");
    let img = dynamic_img.to_rgba8();
    let (width, height) = img.dimensions();
    let gradient_rgb = select_gradient_color(gradient_variant, width, height, &img);
    let img = create_overlay_image(width, height, gradient_rgb, img, fade);
    let duration = start.elapsed();
    println!("create image took: {:?}", duration);
    img
}

fn select_gradient_color(
    select: GradientColorType,
    width: u32,
    height: u32,
    img: &image::ImageBuffer<Rgba<u8>, Vec<u8>>,
) -> Srgb<u8> {
    match select {
        GradientColorType::Dominant => {
            let flat: Vec<u8> = img.pixels().flat_map(|p| p.0[..3].to_vec()).collect();
            calculate_dominant_color(&flat)
        }
        GradientColorType::DominantBottom => {
            let flat: Vec<u8> = (0..width)
                .flat_map(|x| {
                    let pixel = img.get_pixel(x, height - 1); // y = 0 for the first row
                    pixel.0[..3].to_vec() // RGB only
                })
                .collect();
            calculate_dominant_color(&flat)
        }
        GradientColorType::UserSelected(r, g, b) => Srgb::<u8>::new(r, g, b),
    }
}

fn create_overlay_image(
    width: u32,
    height: u32,
    gradient_rgb: Srgb<u8>,
    img: image::ImageBuffer<Rgba<u8>, Vec<u8>>,
    fade: f32,
) -> image::ImageBuffer<Rgba<u8>, Vec<u8>> {
    let mut output = RgbaImage::new(width, height);
    for y in 0..height {
        // bottom - middle -top
        let normalized_y = y as f32 / height as f32;
        let factor = if y as f32 > ((1.0 - 0.4) * height as f32 / 2f32).round() {
            fade
        } else {
            1.0
        };
        // if 0.5 0 at middle, 1 at top/bottom, otherwise shift position toward top/bottom
        let distance_from_middle = (normalized_y - 0.4).abs() * 2.0;
        let alpha = factor * distance_from_middle.powf(2.0);

        // bottom to top
        //let alpha = (y as f32 / height as f32).powf(2.0);
        let overlay = Rgba([
            gradient_rgb.red,
            gradient_rgb.green,
            gradient_rgb.blue,
            (alpha * 255.0) as u8,
        ]);

        for x in 0..width {
            let base = img.get_pixel(x, y);
            let blended = blend_pixels(*base, overlay);
            output.put_pixel(x, y, blended);
        }
    }
    output
}

fn calculate_dominant_color(flat: &Vec<u8>) -> Srgb<u8> {
    let lab: Vec<Lab> = from_component_slice::<Srgb<u8>>(&flat)
        .iter()
        .map(|x| x.into_linear().into_color())
        .collect();

    let kmeans = get_kmeans(1, 10, 1e-5, false, &lab, 42);
    let dominant_lab = kmeans.centroids[0];
    let linear_rgb: Srgb<f32> = dominant_lab.into_color();
    linear_rgb.into_format()
}

fn blend_pixels(base: Rgba<u8>, overlay: Rgba<u8>) -> Rgba<u8> {
    let alpha = overlay[3] as f32 / 255.0;
    let inv_alpha = 1.0 - alpha;
    // use round for the values and not truncate in the convertion
    let r = (overlay[0] as f32 * alpha + base[0] as f32 * inv_alpha).round() as u8;
    let g = (overlay[1] as f32 * alpha + base[1] as f32 * inv_alpha).round() as u8;
    let b = (overlay[2] as f32 * alpha + base[2] as f32 * inv_alpha).round() as u8;

    Rgba([r, g, b, 255])
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::Method::GET;
    use httpmock::MockServer;
    use image::codecs::png::PngEncoder;
    use image::{ExtendedColorType, ImageBuffer, ImageEncoder, Rgba, RgbaImage};
    use palette::Srgb;

    #[tokio::test]
    async fn test_generate_from_url_with_mock() {
        let server = MockServer::start();
        // Create a small in-memory PNG image
        let img = ImageBuffer::<Rgba<u8>, _>::from_pixel(2, 2, Rgba([255, 0, 0, 255]));
        let mut buf = Vec::new();
        {
            let encoder = PngEncoder::new(&mut buf);
            encoder
                .write_image(
                    &img.as_raw().as_slice(), // raw pixel data
                    img.width(),
                    img.height(),
                    ExtendedColorType::Rgba8,
                )
                .unwrap();
        }

        // Mock the HTTP GET request
        server.mock(|when, then| {
            when.method(GET).path("/test-image");
            then.status(200)
                .header("Content-Type", "image/png")
                .body(buf.clone());
        });

        let url = format!("{}/test-image", server.url(""));
        let result = generate_from_url(url, GradientColorType::UserSelected(50, 50, 50), 1.0).await;

        // Assert the output file exists and is a valid image
        assert_eq!(result.dimensions(), (2, 2));
    }

    #[test]
    fn test_select_gradient_color_dominant() {
        let img = dummy_image(2, 2, Rgba([10, 20, 30, 255]));
        let result = select_gradient_color(GradientColorType::Dominant, 2, 2, &img);
        assert_eq!(result, Srgb::new(10, 20, 30));
    }

    #[test]
    fn test_select_gradient_color_dominant_bottom() {
        let mut img = dummy_image(2, 2, Rgba([0, 0, 0, 255]));
        img.put_pixel(0, 1, Rgba([100, 150, 200, 255]));
        img.put_pixel(1, 1, Rgba([100, 150, 200, 255]));

        let result = select_gradient_color(GradientColorType::DominantBottom, 2, 2, &img);
        assert_eq!(result, Srgb::new(100, 150, 200));
    }

    #[test]
    fn test_select_gradient_color_user_selected() {
        let result = select_gradient_color(
            GradientColorType::UserSelected(1, 2, 3),
            0,
            0,
            &dummy_image(1, 1, Rgba([0, 0, 0, 255])),
        );
        assert_eq!(result, Srgb::new(1, 2, 3));
    }

    #[test]
    fn test_create_overlay_image_dimensions() {
        let width = 4;
        let height = 4;
        let base_color = Rgba([100, 100, 100, 255]);
        let dominant_color = Srgb::new(255, 0, 0); // Red

        let img = dummy_image(width, height, base_color);
        let result = create_overlay_image(width, height, dominant_color, img, 1.0);

        assert_eq!(result.width(), width);
        assert_eq!(result.height(), height);
    }

    #[test]
    fn test_create_overlay_image_blending() {
        let width = 1;
        let height = 2;
        let base_color = Rgba([0, 0, 0, 255]);
        let dominant_color = Srgb::new(255, 0, 0); // Red

        let img = dummy_image(width, height, base_color);
        let result = create_overlay_image(width, height, dominant_color, img, 1.0);

        // Check that the output pixel is not the same as the base (i.e., blending occurred)
        let top_pixel = result.get_pixel(0, 0);
        let bottom_pixel = result.get_pixel(0, 1);

        assert_ne!(top_pixel, &base_color);
        assert_ne!(bottom_pixel, &base_color);
    }

    #[test]
    fn test_calculate_dominant_color_single_color() {
        let red_pixel = vec![255, 0, 0];
        let flat: Vec<u8> = red_pixel.repeat(10);
        let dominant = calculate_dominant_color(&flat);
        assert_eq!(dominant, Srgb::new(255, 0, 0));
    }

    #[test]
    fn test_blend_pixels_half_alpha() {
        let base = Rgba([100, 100, 100, 255]);
        let overlay = Rgba([200, 50, 0, 128]); // 50% alpha

        let result = blend_pixels(base, overlay);
        let expected = Rgba([
            ((200f32 * 0.5 + 100f32 * 0.5).round() as u8),
            ((50f32 * 0.5 + 100f32 * 0.5).round() as u8),
            ((0f32 * 0.5 + 100f32 * 0.5).round() as u8),
            255,
        ]);

        assert_eq!(result, expected);
    }

    #[test]
    fn test_blend_pixels_full_alpha() {
        let base = Rgba([100, 100, 100, 255]);
        let overlay = Rgba([255, 0, 0, 255]); // fully opaque

        let result = blend_pixels(base, overlay);
        let expected = Rgba([255, 0, 0, 255]);

        assert_eq!(result, expected);
    }

    #[test]
    fn test_blend_pixels_zero_alpha() {
        let base = Rgba([100, 100, 100, 255]);
        let overlay = Rgba([255, 0, 0, 0]); // fully transparent

        let result = blend_pixels(base, overlay);
        let expected = Rgba([100, 100, 100, 255]);

        assert_eq!(result, expected);
    }

    fn dummy_image(width: u32, height: u32, color: Rgba<u8>) -> RgbaImage {
        let mut img = RgbaImage::new(width, height);
        for y in 0..height {
            for x in 0..width {
                img.put_pixel(x, y, color);
            }
        }
        img
    }
}
