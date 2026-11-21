#![forbid(unsafe_code)]

use axum::extract::Form;
use axum::http::StatusCode;
use axum::response::Html;
use axum::routing::{get, post};
use axum::Router;
use bill_echo::{analyze_csv, escape_html, format_money, render_rows, ChargeSign};
use serde::Deserialize;
use std::net::SocketAddr;

const SAMPLE: &str = "date,description,amount\n2026-01-03,STREAMBOX 9821,-9.99\n2026-02-03,STREAMBOX 1734,-9.99\n2026-03-03,STREAMBOX 6612,-10.99\n2026-04-03,STREAMBOX 4319,-10.99\n2026-01-15,CLOUD BOOKS,-18.00\n2026-02-15,CLOUD BOOKS,-18.00\n2026-03-15,CLOUD BOOKS,-18.00\n2026-04-15,CLOUD BOOKS,-18.00\n2026-03-19,NEIGHBOURHOOD MARKET,-74.26\n";

#[derive(Debug, Deserialize)]
struct AnalyzeForm {
    csv: String,
    charge_sign: ChargeSign,
}

#[tokio::main]
async fn main() {
    let port = std::env::var("PORT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(3000);
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .expect("failed to bind local server");
    println!("Bill Echo is private and local at http://{address}");
    axum::serve(listener, app())
        .await
        .expect("local server stopped unexpectedly");
}

fn app() -> Router {
    Router::new()
        .route("/", get(index))
        .route("/analyze", post(analyze))
        .route("/health", get(|| async { "ok" }))
}

async fn index() -> Html<String> {
    Html(page(SAMPLE, ChargeSign::Negative, ""))
}

async fn analyze(Form(form): Form<AnalyzeForm>) -> (StatusCode, Html<String>) {
    match analyze_csv(&form.csv, form.charge_sign) {
        Ok(analysis) => {
            let annual = analysis
                .recurring
                .iter()
                .map(|series| series.annualized_cents)
                .sum::<i64>();
            let creep = analysis
                .recurring
                .iter()
                .filter(|series| series.price_creep)
                .count();
            let results = if analysis.recurring.is_empty() {
                "<section class=\"empty\"><h2>No recurring series yet</h2><p>Bill Echo needs at least three consistently timed charges from the same normalized merchant.</p></section>".to_owned()
            } else {
                format!(
                    "<section class=\"summary\"><div><strong>{}</strong><span>recurring merchants</span></div><div><strong>{}</strong><span>estimated yearly</span></div><div><strong>{}</strong><span>price-creep flags</span></div></section><div class=\"table-wrap\"><table><thead><tr><th>Merchant</th><th>Typical charge</th><th>Trend</th><th>Next expected</th><th>Confidence</th><th>Yearly</th></tr></thead><tbody>{}</tbody></table></div><p class=\"footnote\">Parsed {} charges across {} normalized merchants. Estimates are observations, not billing guarantees.</p>",
                    analysis.recurring.len(),
                    format_money(annual),
                    creep,
                    render_rows(&analysis),
                    analysis.parsed_charges,
                    analysis.merchant_count,
                )
            };
            (
                StatusCode::OK,
                Html(page(&form.csv, form.charge_sign, &results)),
            )
        }
        Err(error) => {
            let message = format!(
                "<section class=\"error\"><strong>Could not analyze this export.</strong><span>{}</span></section>",
                escape_html(&error.to_string())
            );
            (
                StatusCode::UNPROCESSABLE_ENTITY,
                Html(page(&form.csv, form.charge_sign, &message)),
            )
        }
    }
}

fn page(csv: &str, charge_sign: ChargeSign, results: &str) -> String {
    let positive_selected = if charge_sign == ChargeSign::Positive {
        "selected"
    } else {
        ""
    };
    let negative_selected = if charge_sign == ChargeSign::Negative {
        "selected"
    } else {
        ""
    };
    format!(
        r#"<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>Bill Echo — hear what keeps charging</title>
<style>
:root{{--ink:#17211b;--muted:#627068;--paper:#f5f2e9;--card:#fffdf7;--accent:#e84a2a;--green:#16735a;--line:#d9d5c9}}*{{box-sizing:border-box}}body{{margin:0;background:var(--paper);color:var(--ink);font-family:ui-monospace,SFMono-Regular,Menlo,monospace}}main{{max-width:1120px;margin:auto;padding:64px 24px 80px}}header{{display:grid;grid-template-columns:1.2fr .8fr;gap:48px;align-items:end;margin-bottom:40px}}.eyebrow{{color:var(--accent);font-weight:800;letter-spacing:.12em;text-transform:uppercase}}h1{{font-family:Georgia,serif;font-size:clamp(3.2rem,9vw,7rem);line-height:.82;letter-spacing:-.06em;margin:.2em 0}}header p{{font-size:1.05rem;line-height:1.65;color:var(--muted);max-width:42ch}}.privacy{{border-left:4px solid var(--green);padding:10px 16px;background:#e8f2eb;color:#245c49}}form{{background:var(--card);border:1px solid var(--line);box-shadow:8px 8px 0 var(--ink);padding:24px}}.form-head{{display:flex;align-items:end;justify-content:space-between;gap:18px;margin-bottom:14px}}label{{font-weight:800}}select,button,textarea{{font:inherit}}select{{padding:9px 12px;border:1px solid var(--ink);background:white}}textarea{{display:block;width:100%;min-height:245px;resize:vertical;border:1px solid var(--line);background:#fcfbf5;padding:16px;line-height:1.5}}button{{margin-top:16px;background:var(--accent);color:white;border:0;padding:14px 24px;font-weight:900;cursor:pointer;box-shadow:4px 4px 0 var(--ink)}}button:hover{{transform:translate(2px,2px);box-shadow:2px 2px 0 var(--ink)}}.hint{{font-size:.82rem;color:var(--muted)}}.summary{{display:grid;grid-template-columns:repeat(3,1fr);gap:1px;background:var(--ink);border:1px solid var(--ink);margin-top:44px}}.summary div{{background:var(--card);padding:24px}}.summary strong{{display:block;font-family:Georgia,serif;font-size:2rem}}.summary span,small{{display:block;color:var(--muted);margin-top:5px}}.table-wrap{{overflow:auto;margin-top:22px;background:var(--card);border:1px solid var(--line)}}table{{border-collapse:collapse;width:100%;min-width:850px}}th,td{{text-align:left;padding:16px;border-bottom:1px solid var(--line)}}th{{font-size:.75rem;text-transform:uppercase;letter-spacing:.08em;background:#ece8db}}.badge{{display:inline-block;border-radius:100px;padding:5px 9px;font-size:.75rem;font-weight:800}}.rise{{background:#ffe2d9;color:#a82910}}.fall{{background:#ddf4e9;color:#116347}}.steady{{background:#ece8db}}.footnote,.empty{{color:var(--muted)}}.empty,.error{{margin-top:30px;padding:22px;border:1px solid var(--line);background:var(--card)}}.error{{border-color:#c5351c;background:#fff0eb;display:flex;gap:12px;flex-direction:column}}footer{{margin-top:52px;border-top:1px solid var(--line);padding-top:18px;color:var(--muted);font-size:.82rem}}@media(max-width:760px){{main{{padding-top:36px}}header{{grid-template-columns:1fr;gap:10px}}.summary{{grid-template-columns:1fr}}.form-head{{align-items:start;flex-direction:column}}}}
</style></head><body><main><header><div><div class="eyebrow">Local recurring-charge detector</div><h1>Bill<br>Echo.</h1></div><div><p>Hear what keeps charging. Paste a bank CSV and find steady subscriptions, changing prices, next expected dates, and yearly cost.</p><div class="privacy">Your CSV stays in this process. Bill Echo has no database, analytics, or external requests.</div></div></header>
<form method="post" action="/analyze"><div class="form-head"><div><label for="csv">Bank CSV</label><div class="hint">Required columns: date, description, amount</div></div><label>Charges are <select name="charge_sign"><option value="negative" {negative_selected}>negative amounts</option><option value="positive" {positive_selected}>positive amounts</option></select></label></div><textarea id="csv" name="csv" spellcheck="false">{}</textarea><button type="submit">Find the echoes →</button></form>{results}
<footer>Bill Echo recognizes weekly, biweekly, monthly, quarterly, and yearly patterns with at least three observations. Review candidates against your statements before acting.</footer></main></body></html>"#,
        escape_html(csv)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    #[tokio::test]
    async fn home_page_is_available() {
        let response = app()
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn health_check_is_available() {
        let response = app()
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
}
