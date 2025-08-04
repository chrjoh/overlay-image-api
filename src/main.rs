use actix_web::error::QueryPayloadError;
use actix_web::{App, HttpResponse, HttpServer, middleware, web};
use async_trait::async_trait;
use image::{ImageBuffer, Rgba};
use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::io::Cursor;
use std::str::FromStr;
use std::sync::Arc;

mod overlay;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
enum GradientType {
    Dominant,
    DominantBottom,
    UserDefined,
}
#[derive(Serialize, Deserialize, Debug, Clone)]
struct Fade(f32);

impl FromStr for Fade {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let v = s.parse::<f32>().map_err(|_| "Invalid fade")?;
        if v > 1.0 || v < 0.0 {
            return Err("Allowed values are 0.0 to 1.0".to_string());
        }
        Ok(Fade(v))
    }
}
impl fmt::Display for Fade {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.2}", self.0)
    }
}
impl PartialEq for Fade {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Rgb(u8, u8, u8);

impl FromStr for Rgb {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.split(',').collect();
        if parts.len() != 3 {
            return Err("Expected format: R,G,B".into());
        }
        let r = parts[0]
            .trim()
            .parse::<u8>()
            .map_err(|_| "Invalid R value")?;
        let g = parts[1]
            .trim()
            .parse::<u8>()
            .map_err(|_| "Invalid G value")?;
        let b = parts[2]
            .trim()
            .parse::<u8>()
            .map_err(|_| "Invalid B value")?;

        Ok(Rgb(r, g, b))
    }
}

impl PartialEq for Rgb {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0 && self.1 == other.1 && self.2 == other.2
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct ImageQuery {
    url: String,
    gradient_variant: GradientType,
    #[serde(default, deserialize_with = "option_from_str_deserialize")]
    rgb: Option<Rgb>,
    #[serde(default, deserialize_with = "option_from_str_deserialize")]
    fade: Option<Fade>,
}

fn option_from_str_deserialize<'a, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'a>,
    T: FromStr,
    T::Err: std::fmt::Display,
{
    let opt = Option::<String>::deserialize(deserializer)?;
    match opt {
        Some(s) => T::from_str(&s).map(Some).map_err(de::Error::custom),
        None => Ok(None),
    }
}

#[async_trait]
pub trait ImageGenerator: Send + Sync {
    async fn generate_from_url(
        &self,
        url: String,
        gradient_variant: overlay::GradientColorType,
        fade: f32,
    ) -> ImageBuffer<Rgba<u8>, Vec<u8>>;
}

pub struct RealImageGenerator;

#[async_trait]
impl ImageGenerator for RealImageGenerator {
    async fn generate_from_url(
        &self,
        url: String,
        gradient_variant: overlay::GradientColorType,
        fade: f32,
    ) -> ImageBuffer<Rgba<u8>, Vec<u8>> {
        overlay::generate_from_url(url, gradient_variant, fade).await
    }
}

async fn image_handler(
    req: actix_web::HttpRequest,
    generator: web::Data<dyn ImageGenerator>,
) -> HttpResponse {
    let query_string = req.query_string();
    let parsed_query = web::Query::<ImageQuery>::from_query(query_string);

    let query = match parsed_query {
        Ok(q) => q.into_inner(),
        Err(e) => {
            let msg = match &e {
                QueryPayloadError::Deserialize(inner) => inner.to_string(),
                _ => e.to_string(),
            };
            return HttpResponse::BadRequest().body(format!("Invalid query: {}", msg));
        }
    };
    let gradient_variant = match query.gradient_variant {
        GradientType::Dominant => overlay::GradientColorType::Dominant,
        GradientType::DominantBottom => overlay::GradientColorType::DominantBottom,
        GradientType::UserDefined => {
            if let Some(rgb) = query.rgb {
                overlay::GradientColorType::UserSelected(rgb.0, rgb.1, rgb.2)
            } else {
                return HttpResponse::BadRequest()
                    .body("Missing mandatory rgb values for user defined gradient");
            }
        }
    };
    let fade_value = query.fade.unwrap_or(Fade(1.0)).0;
    let img = generator
        .generate_from_url(query.url, gradient_variant, fade_value)
        .await;

    // Encode the image to PNG
    let mut buf = Cursor::new(Vec::new());
    match img.write_to(&mut buf, image::ImageFormat::Png) {
        Ok(_) => {
            let png_data = buf.into_inner();
            HttpResponse::Ok().content_type("image/png").body(png_data)
        }
        Err(e) => {
            HttpResponse::InternalServerError().body(format!("Failed to encode image: {}", e))
        }
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    let generator: Arc<dyn ImageGenerator> = Arc::new(RealImageGenerator);

    log::info!("starting HTTP server at http://localhost:8080");

    HttpServer::new(move || {
        App::new()
            // enable logger
            .wrap(middleware::Logger::default())
            .app_data(web::Data::from(generator.clone()))
            .service(web::resource("/image").route(web::get().to(image_handler)))
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}

#[cfg(test)]
mod test {
    use super::*;
    use actix_web::body::to_bytes;
    use actix_web::test::TestRequest;
    use std::sync::Arc;

    pub struct MockImageGenerator;

    #[async_trait]
    impl ImageGenerator for MockImageGenerator {
        async fn generate_from_url(
            &self,
            _url: String,
            _gradient_variant: overlay::GradientColorType,
            _fade: f32,
        ) -> ImageBuffer<Rgba<u8>, Vec<u8>> {
            ImageBuffer::from_pixel(1, 1, Rgba([255, 0, 0, 255]))
        }
    }
    #[test]
    fn test_fade_from_str_valid() {
        assert_eq!(Fade::from_str("0.5").unwrap(), Fade(0.5));
        assert_eq!(Fade::from_str("1.0").unwrap(), Fade(1.0));
        assert_eq!(Fade::from_str("0.0").unwrap(), Fade(0.0));
    }

    #[test]
    fn test_fade_from_str_invalid() {
        assert!(Fade::from_str("abc").is_err());
        assert!(Fade::from_str("-0.1").is_err());
        assert!(Fade::from_str("1.1").is_err());
    }

    #[test]
    fn test_fade_display() {
        let f = Fade(0.12345);
        assert_eq!(format!("{}", f), "0.12");
    }

    #[test]
    fn test_rgb_from_str_valid() {
        assert_eq!(Rgb::from_str("255,0,128").unwrap(), Rgb(255, 0, 128));
        assert_eq!(Rgb::from_str("  10 , 20 , 30 ").unwrap(), Rgb(10, 20, 30));
    }

    #[test]
    fn test_rgb_from_str_invalid() {
        assert!(Rgb::from_str("255,0").is_err());
        assert!(Rgb::from_str("255,0,abc").is_err());
        assert!(Rgb::from_str("255,0,256").is_err()); // 256 is out of u8 range
    }

    #[test]
    fn test_gradient_type_serialization() {
        let g = GradientType::DominantBottom;
        let json = serde_json::to_string(&g).unwrap();
        assert_eq!(json, "\"DominantBottom\"");

        let parsed: GradientType = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, GradientType::DominantBottom);
    }

    #[test]
    fn test_image_query_deserialization() {
        let json = r#"{
            "url": "https://example.com/image.jpg",
            "gradient_variant": "UserDefined",
            "rgb": "255,255,255",
            "fade": "0.5"
        }"#;

        let query: ImageQuery = serde_json::from_str(json).unwrap();
        assert_eq!(query.url, "https://example.com/image.jpg");
        assert_eq!(query.gradient_variant, GradientType::UserDefined);
        assert_eq!(query.rgb, Some(Rgb(255, 255, 255)));
        assert_eq!(query.fade, Some(Fade(0.5)));
    }

    #[test]
    fn test_image_query_missing_optional_fields() {
        let json = r#"{
            "url": "https://example.com/image.jpg",
            "gradient_variant": "Dominant"
        }"#;

        let query: ImageQuery = serde_json::from_str(json).unwrap();
        assert_eq!(query.rgb, None);
        assert_eq!(query.fade, None);
    }

    #[actix_web::test]
    async fn test_image_handler_with_injected_mock() {
        let generator: web::Data<dyn ImageGenerator> =
            web::Data::from(Arc::new(MockImageGenerator) as Arc<dyn ImageGenerator>);

        let req = test::TestRequest::get()
            .uri("/image?url=https://example.com/image.jpg&gradient_variant=Dominant&fade=0.5")
            .to_http_request();

        let resp = image_handler(req, generator).await;

        assert_eq!(resp.status(), actix_web::http::StatusCode::OK);

        let body_bytes = to_bytes(resp.into_body()).await.unwrap();

        assert!(body_bytes.starts_with(&[0x89, b'P', b'N', b'G']));
    }
}
