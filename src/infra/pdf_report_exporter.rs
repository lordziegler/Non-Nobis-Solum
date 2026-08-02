//! `ReportExporter` that writes a PDF, with no new dependency.
//!
//! The report is already a column-aligned monospace document — the same
//! lines the CLI prints — so the whole job is paginating them into a PDF
//! text stream in one of the base-14 fonts every reader ships. That is
//! ~80 lines here against a PDF crate (and its font machinery) or a shell
//! out to a converter the user may not have installed.
//!
//! ponytail: text-only, one font, no images, no links. The moment this
//! needs a logo or a chart, replace the writer rather than growing it.

use std::path::Path;

use crate::core::domain::{DomainError, FertilizerRecommendationReport};
use crate::core::ports::ReportExporter;
use crate::infra::report_renderer;

/// A4 in PostScript points, and Courier at a size where the renderer's
/// 96-column lines fit between the margins.
const PAGE_WIDTH: f64 = 595.28;
const PAGE_HEIGHT: f64 = 841.89;
const MARGIN: f64 = 36.0;
const FONT_SIZE: f64 = 8.0;
const LEADING: f64 = 10.0;

pub struct PdfReportExporter;

impl ReportExporter for PdfReportExporter {
    fn export(&self, report: &FertilizerRecommendationReport, destination: &Path) -> Result<(), DomainError> {
        let bytes = build_pdf(&report_renderer::render(report));
        if let Some(parent) = destination.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).map_err(|e| DomainError::DataSource(format!("{}: {e}", parent.display())))?;
        }
        std::fs::write(destination, bytes)
            .map_err(|e| DomainError::DataSource(format!("{}: {e}", destination.display())))
    }
}

fn lines_per_page() -> usize {
    (((PAGE_HEIGHT - 2.0 * MARGIN) / LEADING).floor() as usize).max(1)
}

/// Object numbering: 1 catalog, 2 page tree, 3 font, then two objects per
/// page (the page itself and its content stream).
fn build_pdf(lines: &[String]) -> Vec<u8> {
    let pages: Vec<&[String]> = lines.chunks(lines_per_page()).collect();
    let pages = if pages.is_empty() { vec![&[][..]] } else { pages };
    let page_count = pages.len();

    let page_ids: Vec<usize> = (0..page_count).map(|index| 4 + index * 2).collect();
    let kids: Vec<String> = page_ids.iter().map(|id| format!("{id} 0 R")).collect();

    let mut objects: Vec<Vec<u8>> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        format!("<< /Type /Pages /Kids [{}] /Count {page_count} >>", kids.join(" ")).into_bytes(),
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Courier /Encoding /WinAnsiEncoding >>".to_vec(),
    ];

    for (index, page) in pages.iter().enumerate() {
        let content_id = page_ids[index] + 1;
        objects.push(
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {PAGE_WIDTH:.2} {PAGE_HEIGHT:.2}] \
                 /Resources << /Font << /F1 3 0 R >> >> /Contents {content_id} 0 R >>"
            )
            .into_bytes(),
        );
        let stream = text_stream(page);
        let mut content = format!("<< /Length {} >>\nstream\n", stream.len()).into_bytes();
        content.extend_from_slice(&stream);
        content.extend_from_slice(b"\nendstream");
        objects.push(content);
    }

    assemble(&objects)
}

fn text_stream(lines: &[String]) -> Vec<u8> {
    let top = PAGE_HEIGHT - MARGIN - FONT_SIZE;
    let mut stream = format!("BT\n/F1 {FONT_SIZE} Tf\n{LEADING} TL\n{MARGIN:.2} {top:.2} Td\n").into_bytes();
    for line in lines {
        stream.push(b'(');
        stream.extend_from_slice(&escape(line));
        stream.extend_from_slice(b") Tj\nT*\n");
    }
    stream.extend_from_slice(b"ET");
    stream
}

/// PDF string escaping, plus the encoding step.
///
/// The base-14 fonts are single-byte, so the text goes out as WinAnsi.
/// Latin-1 passes through unchanged (which is what keeps `edáfico` and
/// `agrícola` readable); the punctuation WinAnsi keeps in its C1 block sits
/// at a different code point from its Unicode one and is mapped here.
/// Everything else — box drawing, arrows — becomes one ASCII character,
/// never zero and never two: the report is column-aligned, and a
/// substitution of a different width would shift every field after it.
fn escape(line: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(line.len());
    for character in line.chars() {
        match character {
            '\\' => out.extend_from_slice(b"\\\\"),
            '(' => out.extend_from_slice(b"\\("),
            ')' => out.extend_from_slice(b"\\)"),
            // WinAnsi's C1 punctuation, at its WinAnsi code point.
            '…' => out.push(0x85),
            '–' => out.push(0x96),
            '—' => out.push(0x97),
            '‘' => out.push(0x91),
            '’' => out.push(0x92),
            '“' => out.push(0x93),
            '”' => out.push(0x94),
            '•' => out.push(0x95),
            '─' | '═' | '━' | '←' | '↑' | '→' | '↓' | '−' => out.push(b'-'),
            c if (c as u32) < 32 => out.push(b' '),
            // Latin-1, which WinAnsi agrees with above 0xA0.
            c if (c as u32) >= 32 && (c as u32) < 256 => out.push(c as u8),
            _ => out.push(b'?'),
        }
    }
    out
}

/// Header, body, cross-reference table, trailer.
fn assemble(objects: &[Vec<u8>]) -> Vec<u8> {
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::with_capacity(objects.len());

    for (index, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n", index + 1).as_bytes());
        pdf.extend_from_slice(object);
        pdf.extend_from_slice(b"\nendobj\n");
    }

    let xref_offset = pdf.len();
    pdf.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
    for offset in &offsets {
        pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    pdf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pdf_is_structurally_complete_and_paginates() {
        let lines: Vec<String> = (0..200).map(|i| format!("line {i} · with (parens) and a \\ backslash")).collect();
        let pdf = build_pdf(&lines);
        let text = String::from_utf8_lossy(&pdf);

        assert!(text.starts_with("%PDF-1.4"));
        assert!(text.trim_end().ends_with("%%EOF"));
        assert!(text.contains("/Type /Catalog"));
        // 200 lines do not fit on one A4 page at 10pt leading.
        assert!(lines.len() > lines_per_page());
        let page_count = text.matches("/Type /Page ").count();
        assert_eq!(page_count, lines.len().div_ceil(lines_per_page()));
        assert!(text.contains(&format!("/Count {page_count}")));
        // Every object the xref promises actually exists.
        let objects = text.matches(" 0 obj").count();
        assert!(text.contains(&format!("/Size {}", objects + 1)));
        // The escaping ran: a bare "(" inside a string would break the file.
        assert!(text.contains("\\(parens\\)"));
        assert!(!text.contains("·"), "non-WinAnsi characters must be transliterated");
    }

    #[test]
    fn an_empty_report_still_produces_one_valid_page() {
        let pdf = build_pdf(&[]);
        let text = String::from_utf8_lossy(&pdf);
        assert_eq!(text.matches("/Type /Page ").count(), 1);
        assert!(text.contains("/Count 1"));
    }
}
