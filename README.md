## Rust testing with actix and creating overlays for images

This service allows you to generate image overlays using different gradient strategies. It is built using Rust and Actix, and can be significantly optimized by compiling in release mode.

## Performance Tip

To speed up image generation, **build the service in release mode**:

```bash
cargo build --release
```

## API Endpoint

**GET** `/image`

Generates a new image with an overlay based on the specified parameters.

### Query Parameters

| Parameter          | Type   | Required | Description                                                                    |
| ------------------ | ------ | -------- | ------------------------------------------------------------------------------ |
| `url`              | string | Yes      | URL-encoded link to the source image.                                          |
| `gradient_variant` | enum   | Yes      | Determines how the overlay gradient is calculated.                             |
| `rgb`              | string | No       | Comma-separated RGB values (`r,g,b`) of type `u8`. Required for `UserDefined`. |
| `fade`             | float  | No       | Value between `0.0` and `1.0` to control overlay transparency.                 |

## Gradient Variants

- `Dominant`: Uses the most dominant color from the entire image.
- `DominantBottom`: Uses the most dominant color from the bottom row of the image.
- `UserDefined`: Uses a user-specified RGB color. Requires the `rgb` parameter.

## Example Request

```http
GET /image?url=https:%2F%2Fimg.example.com%2Fimage.jpg&gradient_variant=UserDefined&rgb=50,50,150&fade=0.5
```

This request applies a semi-transparent overlay using the RGB color (50, 50, 150) to the image at the specified URL.
