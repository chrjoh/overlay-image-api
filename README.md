## Rust testing with actix and creating overlays for images

### Example

Parameters

- url: url encoded url to an image
- gradient_variant: select one of Dominant, DominantBottom or UserDefined
  - select how the overlay should be calculated
- rgb: r,g,b values of type u8 this is needed if UserDefined is used
- fade: float 0.0-1.0 if the overlay should be more transparent

```
http://localhost:8080/image?url=https:%2F%2Fimg.example.com/image.jpg&gradient_variant=UserDefined&rgb=50,50,150
```
