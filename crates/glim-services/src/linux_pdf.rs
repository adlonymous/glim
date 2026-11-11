//! On-demand PDF rasterization through Poppler, one page per cancellable job.
use crate::work::{self, Cancellation};
use anyhow::{Context, Result, ensure};
use std::{
    fs::File,
    path::Path,
    process::{Command, Stdio},
};

/// A single PDF page and its document page count; no other pages are decoded.
pub struct PdfPagePreview {
    pub png: Vec<u8>,
    pub pages: usize,
}

fn run_poppler(command: &mut Command, directory: &Path, cancel: &Cancellation) -> Result<Vec<u8>> {
    cancel.check()?;
    let output = directory.join("stdout");
    let errors = directory.join("stderr");
    command
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(File::create(&output)?)
        .stderr(File::create(&errors)?);
    let status = work::run(command, cancel)
        .context("PDF preview requires Poppler (pdfinfo and pdftoppm)")?;
    ensure!(
        status.success(),
        "PDF preview failed: {}",
        std::fs::read_to_string(errors)?
    );
    cancel.check()?;
    Ok(std::fs::read(output)?)
}

/// Render a PDF page at the requested display resolution without limiting file size.
pub fn render_pdf_page(
    path: &Path,
    page: usize,
    resolution: u32,
    cancel: &Cancellation,
) -> Result<PdfPagePreview> {
    ensure!(
        page > 0 && resolution > 0,
        "Invalid PDF preview dimensions or page"
    );
    let path = path.canonicalize()?;
    let directory = tempfile::tempdir()?;
    let info = run_poppler(Command::new("pdfinfo").arg(&path), directory.path(), cancel)?;
    let info = String::from_utf8(info)?;
    let pages = info
        .lines()
        .find_map(|line| line.strip_prefix("Pages:"))
        .context("PDF page count unavailable")?
        .trim()
        .parse::<usize>()?;
    ensure!(page <= pages, "PDF page is outside the document");
    let prefix = directory.path().join("page");
    run_poppler(
        Command::new("pdftoppm")
            .args([
                "-f",
                &page.to_string(),
                "-l",
                &page.to_string(),
                "-singlefile",
                "-scale-to",
                &resolution.to_string(),
                "-png",
            ])
            .arg(&path)
            .arg(&prefix),
        directory.path(),
        cancel,
    )?;
    Ok(PdfPagePreview {
        png: std::fs::read(prefix.with_extension("png"))?,
        pages,
    })
}

/// Retains a generated HTML file while an external preview is in use.
pub struct ExternalHtmlPreview {
    _directory: tempfile::TempDir,
}

/// Open a local document through the Linux desktop, retaining unsaved HTML separately.
pub fn open_external_preview(
    path: &Path,
    html: Option<&str>,
) -> Result<Option<ExternalHtmlPreview>> {
    let directory = html.map(|_| tempfile::tempdir()).transpose()?;
    let destination = if let Some(directory) = &directory {
        let destination = directory.path().join("preview.html");
        std::fs::write(&destination, html.unwrap())?;
        destination
    } else {
        path.canonicalize()?
    };
    let output = Command::new("xdg-open")
        .arg(&destination)
        .output()
        .context("External preview requires xdg-utils")?;
    ensure!(
        output.status.success(),
        "Cannot open external preview: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(directory.map(|directory| ExternalHtmlPreview {
        _directory: directory,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancellation_stops_before_launching_poppler() -> Result<()> {
        let (cancel, guard) = Cancellation::task();
        drop(guard);
        let directory = tempfile::tempdir()?;
        assert!(
            run_poppler(
                &mut Command::new("must-not-be-launched"),
                directory.path(),
                &cancel
            )
            .is_err()
        );
        Ok(())
    }

    #[test]
    fn pdf_pages_are_rendered_on_demand() -> Result<()> {
        use std::io::Write;
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("two pages.pdf");
        let objects = [
            "<< /Type /Catalog /Pages 2 0 R >>",
            "<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 200] /Resources << >> /Contents 5 0 R >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 100] /Resources << >> /Contents 5 0 R >>",
            "<< /Length 0 >>\nstream\n\nendstream",
        ];
        let mut pdf = Vec::from(&b"%PDF-1.4\n"[..]);
        let mut offsets = Vec::new();
        for (index, object) in objects.iter().enumerate() {
            offsets.push(pdf.len());
            writeln!(pdf, "{} 0 obj\n{object}\nendobj", index + 1)?;
        }
        let xref = pdf.len();
        writeln!(pdf, "xref\n0 6\n0000000000 65535 f ")?;
        for offset in offsets {
            writeln!(pdf, "{offset:010} 00000 n ")?;
        }
        writeln!(
            pdf,
            "trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF"
        )?;
        std::fs::write(&path, pdf)?;
        let cancel = Cancellation::default();
        let first = render_pdf_page(&path, 1, 200, &cancel)?;
        let second = render_pdf_page(&path, 2, 200, &cancel)?;
        assert_eq!(first.pages, 2);
        assert_eq!(image::load_from_memory(&first.png)?.width(), 100);
        assert_eq!(image::load_from_memory(&second.png)?.width(), 200);
        assert!(render_pdf_page(&path, 3, 200, &cancel).is_err());
        Ok(())
    }

    #[test]
    fn invalid_page_is_rejected_before_reading_file() {
        assert!(
            render_pdf_page(Path::new("/missing.pdf"), 0, 1600, &Cancellation::default()).is_err()
        );
    }
}
