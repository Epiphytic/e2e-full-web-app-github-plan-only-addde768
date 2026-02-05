use actix_web::HttpRequest;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};

use serde::{Deserialize, Serialize};

use crate::AppState;

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub exp: usize,
    pub iat: usize,
}

#[derive(Serialize)]
pub struct JwksResponse {
    pub keys: Vec<JwkJson>,
}

#[derive(Serialize)]
pub struct JwkJson {
    pub kty: String,
    #[serde(rename = "use")]
    pub use_field: String,
    pub kid: String,
    pub alg: String,
    pub n: String,
    pub e: String,
}

pub fn create_token(
    private_key_pem: &[u8],
    sub: &str,
    duration_secs: u64,
    kid: &str,
) -> Result<String, jsonwebtoken::errors::Error> {
    let now = chrono::Utc::now().timestamp() as usize;
    let claims = Claims {
        sub: sub.to_string(),
        iat: now,
        exp: now + duration_secs as usize,
    };

    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some(kid.to_string());

    let key = EncodingKey::from_rsa_pem(private_key_pem)?;
    encode(&header, &claims, &key)
}

pub fn validate_token(
    public_key_pem: &[u8],
    token: &str,
) -> Result<Claims, jsonwebtoken::errors::Error> {
    let key = DecodingKey::from_rsa_pem(public_key_pem)?;
    let mut validation = Validation::new(Algorithm::RS256);
    validation.validate_exp = true;
    validation.leeway = 0;
    let token_data = decode::<Claims>(token, &key, &validation)?;
    Ok(token_data.claims)
}

pub fn extract_token(req: &HttpRequest) -> Option<String> {
    // Check Authorization header
    if let Some(auth_header) = req.headers().get("Authorization") {
        if let Ok(auth_str) = auth_header.to_str() {
            if let Some(token) = auth_str.strip_prefix("Bearer ") {
                return Some(token.to_string());
            }
        }
    }

    // Check cookie
    if let Some(cookie) = req.cookie("token") {
        return Some(cookie.value().to_string());
    }

    None
}

pub fn authenticate(req: &HttpRequest, state: &AppState) -> Result<Claims, String> {
    let token = extract_token(req).ok_or("No authentication token provided")?;
    validate_token(&state.jwt_public_key, &token).map_err(|e| format!("Invalid token: {}", e))
}

pub fn get_jwks(public_key_pem: &[u8], kid: &str) -> JwksResponse {
    // Parse the PEM to extract the RSA public key components
    let pem_str = String::from_utf8_lossy(public_key_pem);
    let der = pem::parse(pem_str.as_ref()).expect("Failed to parse PEM");
    let public_key_der = der.contents();

    // Parse the SPKI structure to extract n and e
    // RSA public key in SPKI format: SEQUENCE { SEQUENCE { OID, NULL }, BIT STRING { SEQUENCE { INTEGER n, INTEGER e } } }
    let (n_bytes, e_bytes) = extract_rsa_components(public_key_der);

    let n = URL_SAFE_NO_PAD.encode(&n_bytes);
    let e = URL_SAFE_NO_PAD.encode(&e_bytes);

    JwksResponse {
        keys: vec![JwkJson {
            kty: "RSA".to_string(),
            use_field: "sig".to_string(),
            kid: kid.to_string(),
            alg: "RS256".to_string(),
            n,
            e,
        }],
    }
}

fn extract_rsa_components(spki_der: &[u8]) -> (Vec<u8>, Vec<u8>) {
    // Simple ASN.1 DER parser for SPKI-wrapped RSA public key
    // This handles the SubjectPublicKeyInfo structure
    let mut pos = 0;

    // Outer SEQUENCE
    assert_eq!(spki_der[pos], 0x30, "Expected SEQUENCE");
    pos += 1;
    let (_outer_len, consumed) = read_der_length(&spki_der[pos..]);
    pos += consumed;

    // AlgorithmIdentifier SEQUENCE
    assert_eq!(
        spki_der[pos], 0x30,
        "Expected SEQUENCE for AlgorithmIdentifier"
    );
    pos += 1;
    let (algo_len, consumed) = read_der_length(&spki_der[pos..]);
    pos += consumed;
    pos += algo_len; // Skip AlgorithmIdentifier contents

    // BIT STRING containing the public key
    assert_eq!(spki_der[pos], 0x03, "Expected BIT STRING");
    pos += 1;
    let (_bs_len, consumed) = read_der_length(&spki_der[pos..]);
    pos += consumed;
    pos += 1; // Skip unused bits byte

    // Now we're at the inner RSA public key SEQUENCE
    assert_eq!(spki_der[pos], 0x30, "Expected SEQUENCE for RSA public key");
    pos += 1;
    let (_inner_len, consumed) = read_der_length(&spki_der[pos..]);
    pos += consumed;

    // INTEGER n
    assert_eq!(spki_der[pos], 0x02, "Expected INTEGER for n");
    pos += 1;
    let (n_len, consumed) = read_der_length(&spki_der[pos..]);
    pos += consumed;
    let mut n_bytes = spki_der[pos..pos + n_len].to_vec();
    // Remove leading zero byte if present (used for sign)
    if !n_bytes.is_empty() && n_bytes[0] == 0 {
        n_bytes.remove(0);
    }
    pos += n_len;

    // INTEGER e
    assert_eq!(spki_der[pos], 0x02, "Expected INTEGER for e");
    pos += 1;
    let (e_len, consumed) = read_der_length(&spki_der[pos..]);
    pos += consumed;
    let mut e_bytes = spki_der[pos..pos + e_len].to_vec();
    if !e_bytes.is_empty() && e_bytes[0] == 0 {
        e_bytes.remove(0);
    }

    (n_bytes, e_bytes)
}

fn read_der_length(data: &[u8]) -> (usize, usize) {
    if data[0] < 0x80 {
        (data[0] as usize, 1)
    } else {
        let num_bytes = (data[0] & 0x7f) as usize;
        let mut length: usize = 0;
        for i in 0..num_bytes {
            length = (length << 8) | data[1 + i] as usize;
        }
        (length, 1 + num_bytes)
    }
}
