use std::path::PathBuf;
use std::sync::Mutex;

use eframe::egui;

use crate::window_state::Window;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Group {
    FromPdf,
    ToPdf,
    Fix,
}

const GROUPS: [(Group, &str); 3] = [
    (Group::FromPdf, "Convert from PDF"),
    (Group::ToPdf, "Convert to PDF"),
    (Group::Fix, "Shrink and fix"),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Family {
    OutDir,
    OutSlash,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Tool {
    id: &'static str,
    title: &'static str,
    short: &'static str,
    group: Group,
    badge: &'static str,
    colour: [u8; 3],
    accepts: &'static [&'static str],
    several: bool,
    family: Family,
    extra: &'static [&'static str],
}

const PDF: &[&str] = &["application/pdf"];

pub(crate) const PICTURES: &[&str] = &["image/jpeg", "image/png"];

pub(crate) const FOR_THE_CHAT: &[&str] = &["image/jpeg", "image/png", "application/pdf"];

pub(crate) const FOR_THE_WINDOW: &str = "window:";
const BLUE: [u8; 3] = [37, 99, 235];
const GREEN: [u8; 3] = [22, 163, 74];
const ORANGE: [u8; 3] = [234, 88, 12];
const RED: [u8; 3] = [220, 38, 38];
const GREY: [u8; 3] = [71, 85, 105];
const PURPLE: [u8; 3] = [124, 58, 237];

pub(crate) const TOOLS: &[Tool] = &[
    Tool {
        id: "pdf-to-word",
        title: "PDF to Word",
        short: "Word",
        group: Group::FromPdf,
        badge: "W",
        colour: BLUE,
        accepts: PDF,
        several: false,
        family: Family::OutDir,
        extra: &[],
    },
    Tool {
        id: "pdf-to-excel",
        title: "PDF to Excel",
        short: "Excel",
        group: Group::FromPdf,
        badge: "X",
        colour: GREEN,
        accepts: PDF,
        several: false,
        family: Family::OutDir,
        extra: &[],
    },
    Tool {
        id: "pdf-to-powerpoint",
        title: "PDF to PowerPoint",
        short: "PowerPoint",
        group: Group::FromPdf,
        badge: "P",
        colour: ORANGE,
        accepts: PDF,
        several: false,
        family: Family::OutDir,
        extra: &[],
    },
    Tool {
        id: "pdf-to-image",
        title: "PDF to JPG",
        short: "Image",
        group: Group::FromPdf,
        badge: "JPG",
        colour: PURPLE,
        accepts: PDF,
        several: false,
        family: Family::OutSlash,
        extra: &[],
    },
    Tool {
        id: "pdf-to-text",
        title: "PDF to Text",
        short: "Text",
        group: Group::FromPdf,
        badge: "TXT",
        colour: GREY,
        accepts: PDF,
        several: false,
        family: Family::OutDir,
        extra: &[],
    },
    Tool {
        id: "pdf-to-html",
        title: "PDF to HTML",
        short: "Web page",
        group: Group::FromPdf,
        badge: "HTML",
        colour: ORANGE,
        accepts: PDF,
        several: false,
        family: Family::OutDir,
        extra: &[],
    },
    Tool {
        id: "pdf-to-markdown",
        title: "PDF to Markdown",
        short: "Markdown",
        group: Group::FromPdf,
        badge: "MD",
        colour: GREY,
        accepts: PDF,
        several: false,
        family: Family::OutDir,
        extra: &[],
    },
    Tool {
        id: "word-to-pdf",
        title: "Word to PDF",
        short: "Word",
        group: Group::ToPdf,
        badge: "PDF",
        colour: BLUE,
        accepts: &["application/vnd.openxmlformats-officedocument.wordprocessingml.document"],
        several: false,
        family: Family::OutDir,
        extra: &[],
    },
    Tool {
        id: "excel-to-pdf",
        title: "Excel to PDF",
        short: "Excel",
        group: Group::ToPdf,
        badge: "PDF",
        colour: GREEN,
        accepts: &["application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"],
        several: false,
        family: Family::OutDir,
        extra: &[],
    },
    Tool {
        id: "powerpoint-to-pdf",
        title: "PowerPoint to PDF",
        short: "PowerPoint",
        group: Group::ToPdf,
        badge: "PDF",
        colour: ORANGE,
        accepts: &["application/vnd.openxmlformats-officedocument.presentationml.presentation"],
        several: false,
        family: Family::OutDir,
        extra: &[],
    },
    Tool {
        id: "image-to-pdf",
        title: "JPG to PDF",
        short: "Photos",
        group: Group::ToPdf,
        badge: "PDF",
        colour: PURPLE,
        accepts: &["image/jpeg", "image/png", "image/gif"],
        several: true,
        family: Family::OutSlash,
        extra: &[],
    },
    Tool {
        id: "compress-pdf",
        title: "Compress PDF",
        short: "Compress",
        group: Group::Fix,
        badge: "ZIP",
        colour: RED,
        accepts: PDF,
        several: false,
        family: Family::OutSlash,
        extra: &[],
    },
    Tool {
        id: "repair-pdf",
        title: "Repair PDF",
        short: "Repair",
        group: Group::Fix,
        badge: "FIX",
        colour: GREY,
        accepts: PDF,
        several: false,
        family: Family::OutDir,
        extra: &[],
    },
    Tool {
        id: "pdf-to-pdfa",
        title: "PDF to PDF/A",
        short: "PDF/A",
        group: Group::Fix,
        badge: "A",
        colour: RED,
        accepts: PDF,
        several: false,
        family: Family::OutDir,
        extra: &[],
    },
];

enum Job {
    Running(
        &'static Tool,
        std::thread::JoinHandle<Result<Vec<PathBuf>, String>>,
    ),
    Done(&'static Tool, Result<Vec<PathBuf>, String>),
}

static JOB: Mutex<Option<Job>> = Mutex::new(None);

static CHOSEN: Mutex<Option<(String, Vec<PathBuf>)>> = Mutex::new(None);

pub fn chosen_for_tool(id: String, files: Vec<PathBuf>) {
    if let Ok(mut chosen) = CHOSEN.lock() {
        *chosen = Some((id, files));
    }
}

impl Window {
    pub(crate) fn mobile_start(&mut self, ui: &mut egui::Ui, idle: bool) {
        let room = ui.available_width();
        let half = (room - BIG_GAP) / 2.0;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = BIG_GAP;
            if big_button(ui, half, crate::icons::Icon::Open, "Open PDF", true, idle) {
                self.asking_to_open = true;
            }
            if big_button(ui, half, crate::icons::Icon::Scan, "Scan", false, idle) {
                crate::android::scan(false);
            }
        });
        ui.add_space(6.0);
        if ui
            .add_enabled(
                idle,
                egui::Button::new(egui::RichText::new("+  New blank PDF").size(14.0)).frame(false),
            )
            .clicked()
        {
            self.new_document(crate::chrome::A4);
        }
        for (group, heading) in GROUPS {
            ui.add_space(18.0);
            ui.label(egui::RichText::new(heading).size(15.0).strong());
            ui.add_space(8.0);
            let tools: Vec<&Tool> = TOOLS.iter().filter(|tool| tool.group == group).collect();
            let across = ((room + TILE_GAP) / (TILE_SIDE + TILE_GAP))
                .floor()
                .clamp(3.0, 6.0);
            let side = (room - TILE_GAP * (across - 1.0)) / across;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let across = across as usize;
            for row in tools.chunks(across) {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = TILE_GAP;
                    for tool in row {
                        if tool_tile(ui, tool, side, idle) {
                            crate::android::pick_files(tool.accepts, tool.several, tool.id);
                        }
                    }
                });
                ui.add_space(TILE_GAP);
            }
        }
        ui.add_space(12.0);
        if ui
            .add(
                egui::Button::new(
                    egui::RichText::new("About PanPDF and licences")
                        .size(13.0)
                        .color(ui.visuals().weak_text_color()),
                )
                .frame(false),
            )
            .clicked()
        {
            self.showing_about = true;
        }
    }

    pub(crate) fn about_the_app(&mut self, ctx: &egui::Context) {
        if !self.showing_about {
            return;
        }
        let mut close = false;
        let modal = egui::Modal::new(egui::Id::new("about-the-app")).show(ctx, |ui| {
            ui.set_width(crate::side_panel::box_width(ui.ctx(), 560.0));
            ui.heading(format!("PanPDF {}", env!("CARGO_PKG_VERSION")));
            ui.add_space(6.0);
            ui.label(
                "Free and open source, under the GNU Affero General Public License, version 3. \
                 Documents stay on this phone.",
            );
            ui.add_space(4.0);
            ui.hyperlink_to("panpdf.org", "https://panpdf.org");
            ui.hyperlink_to("Source code", "https://github.com/panXDgaming/panpdf.rs");
            ui.add_space(10.0);
            ui.label(egui::RichText::new("Parts made by others").strong());
            ui.add_space(4.0);
            let tall = (ui.ctx().content_rect().height() * 0.55).max(160.0);
            egui::ScrollArea::vertical()
                .max_height(tall)
                .show(ui, |ui| {
                    ui.label(egui::RichText::new(NOTICES).monospace().size(10.5));
                });
            ui.add_space(10.0);
            if ui
                .add_sized([ui.available_width(), 40.0], egui::Button::new("Close"))
                .clicked()
            {
                close = true;
            }
        });
        if close || modal.should_close() {
            self.showing_about = false;
        }
    }

    fn chosen_on_the_phone(&mut self, what: &str, files: &[PathBuf]) {
        match what {
            "place-picture" => self.pictures_chosen_to_place(files),
            "pages-before" | "pages-after" => {
                if let Some(path) = files.first() {
                    self.insert_pages_from(path, what == "pages-before");
                }
            }
            "pictures-before" => self.pictures_chosen(files, Some(true)),
            "pictures-after" => self.pictures_chosen(files, Some(false)),
            "pictures-new" => self.pictures_chosen(files, None),
            "chat-attachment" => {
                for path in files {
                    self.ai.attach_file(path);
                }
            }
            _ => {}
        }
    }

    pub(crate) fn tool_work(&mut self, ctx: &egui::Context) {
        if let Some((id, files)) = CHOSEN.lock().ok().and_then(|mut chosen| chosen.take())
            && !files.is_empty()
        {
            if let Some(tool) = TOOLS.iter().find(|tool| tool.id == id) {
                let handle = std::thread::spawn(move || run_tool(tool, &files));
                if let Ok(mut job) = JOB.lock() {
                    *job = Some(Job::Running(tool, handle));
                }
            } else if let Some(what) = id.strip_prefix(FOR_THE_WINDOW) {
                self.chosen_on_the_phone(what, &files);
            }
        }
        let Ok(mut job) = JOB.lock() else {
            return;
        };
        if let Some(Job::Running(_, handle)) = job.as_ref()
            && handle.is_finished()
            && let Some(Job::Running(tool, handle)) = job.take()
        {
            let result = handle
                .join()
                .unwrap_or_else(|_| Err("the tool stopped".to_owned()));
            *job = Some(Job::Done(tool, result));
        }
        let mut close = false;
        let mut open: Option<PathBuf> = None;
        match job.as_ref() {
            Some(Job::Running(tool, _)) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(200));
                egui::Modal::new(egui::Id::new("tool-running")).show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(format!("{}…", tool.title));
                    });
                });
            }
            Some(Job::Done(tool, result)) => {
                egui::Modal::new(egui::Id::new("tool-done")).show(ctx, |ui| {
                    let wide = (ctx.content_rect().width() - 64.0).clamp(200.0, 380.0);
                    ui.set_width(wide);
                    ui.label(egui::RichText::new(tool.title).strong().size(18.0));
                    ui.add_space(10.0);
                    match result {
                        Ok(files) if !files.is_empty() => {
                            egui::ScrollArea::vertical()
                                .max_height(ctx.content_rect().height() * 0.6)
                                .show(ui, |ui| {
                                    for file in files {
                                        let name = file.file_name().map_or_else(String::new, |n| {
                                            n.to_string_lossy().into_owned()
                                        });
                                        ui.label(
                                            egui::RichText::new(format!("✓  {name}")).size(15.0),
                                        );
                                        ui.add_space(8.0);
                                        if name.to_lowercase().ends_with(".pdf")
                                            && wide_button(ui, wide, "Open", true)
                                        {
                                            open = Some(file.clone());
                                        }
                                        if wide_button(
                                            ui,
                                            wide,
                                            "Save to phone",
                                            open.is_none()
                                                && !name.to_lowercase().ends_with(".pdf"),
                                        ) {
                                            crate::android::export(file);
                                        }
                                        if wide_button(ui, wide, "Share", false)
                                            && let Ok(bytes) = std::fs::read(file)
                                        {
                                            let _ = crate::android::share(&name, &bytes);
                                        }
                                        ui.add_space(12.0);
                                    }
                                });
                        }
                        Ok(_) => {
                            ui.label("Nothing came out of this file.");
                        }
                        Err(why) => {
                            ui.label(why);
                        }
                    }
                    ui.add_space(4.0);
                    close = wide_button(ui, wide, "Close", false);
                });
            }
            None => {}
        }
        if close || open.is_some() {
            *job = None;
        }
        drop(job);
        if let Some(path) = open {
            self.open(&path);
        }
    }
}

const TILE_SIDE: f32 = 80.0;
const TILE_GAP: f32 = 10.0;

const BIG_GAP: f32 = 12.0;

const NOTICES: &str = include_str!("../licences/android.txt");

fn big_button(
    ui: &mut egui::Ui,
    wide: f32,
    icon: crate::icons::Icon,
    name: &str,
    first: bool,
    idle: bool,
) -> bool {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(wide, 104.0), egui::Sense::click());
    let blue = egui::Color32::from_rgb(0, 90, 200);
    let pressed = response.is_pointer_button_down_on();
    let (fill, ink) = if first {
        (
            if pressed {
                blue.gamma_multiply(0.85)
            } else {
                blue
            },
            egui::Color32::WHITE,
        )
    } else {
        let tint = egui::Color32::from_rgba_unmultiplied(0, 90, 200, if pressed { 60 } else { 32 });
        (tint, blue)
    };
    let painter = ui.painter();
    painter.rect_filled(rect, 18.0, fill);
    let glyph = egui::Rect::from_center_size(
        rect.center() - egui::vec2(0.0, 14.0),
        egui::vec2(34.0, 34.0),
    );
    icon.draw_tinted(painter, glyph, ink, true);
    painter.text(
        egui::pos2(rect.center().x, rect.max.y - 16.0),
        egui::Align2::CENTER_BOTTOM,
        name,
        egui::FontId::proportional(16.0),
        ink,
    );
    idle && response.clicked()
}

fn wide_button(ui: &mut egui::Ui, wide: f32, name: &str, first: bool) -> bool {
    let text = egui::RichText::new(name).size(16.0);
    let button = if first {
        egui::Button::new(text.color(egui::Color32::WHITE))
            .fill(egui::Color32::from_rgb(0, 90, 200))
    } else {
        egui::Button::new(text)
    };
    let clicked = ui
        .add_sized(egui::vec2(wide, 48.0), button.corner_radius(12.0))
        .clicked();
    ui.add_space(6.0);
    clicked
}

fn tool_tile(ui: &mut egui::Ui, tool: &Tool, side: f32, idle: bool) -> bool {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(side, side * 0.95), egui::Sense::click());
    let visuals = ui.visuals();
    let fill = if response.hovered() || response.is_pointer_button_down_on() {
        visuals.widgets.hovered.bg_fill
    } else {
        visuals.extreme_bg_color
    };
    let painter = ui.painter();
    painter.rect_filled(rect, 12.0, fill);
    let page = egui::Rect::from_center_size(
        rect.center() - egui::vec2(0.0, side * 0.12),
        egui::vec2(side * 0.36, side * 0.44),
    );
    let colour = egui::Color32::from_rgb(tool.colour[0], tool.colour[1], tool.colour[2]);
    painter.rect_filled(page, 4.0, egui::Color32::WHITE);
    painter.rect_stroke(
        page,
        4.0,
        egui::Stroke::new(1.5, colour),
        egui::StrokeKind::Inside,
    );
    let band = egui::Rect::from_min_max(
        egui::pos2(page.min.x - 4.0, page.center().y),
        egui::pos2(page.max.x + 4.0, page.center().y + page.height() * 0.34),
    );
    painter.rect_filled(band, 3.0, colour);
    painter.text(
        band.center(),
        egui::Align2::CENTER_CENTER,
        tool.badge,
        egui::FontId::proportional(if tool.badge.len() > 2 { 10.0 } else { 13.0 }),
        egui::Color32::WHITE,
    );
    painter.text(
        egui::pos2(rect.center().x, rect.max.y - 8.0),
        egui::Align2::CENTER_BOTTOM,
        tool.short,
        egui::FontId::proportional(12.5),
        if idle {
            visuals.text_color()
        } else {
            visuals.weak_text_color()
        },
    );
    idle && response.clicked()
}

fn run_tool(tool: &Tool, files: &[PathBuf]) -> Result<Vec<PathBuf>, String> {
    let program = crate::android::tools_program().ok_or("the tools are not in this app")?;
    let cache = crate::android::cache_dir().ok_or("no cache folder")?;
    let out = cache.join("tool-out");
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(&out).map_err(|error| error.to_string())?;
    let mut command = std::process::Command::new(&program);
    std::os::unix::process::CommandExt::arg0(&mut command, tool.id);
    command.args(files).args(tool.extra);
    match tool.family {
        Family::OutDir => command.arg("--out-dir").arg(&out),
        Family::OutSlash => command.arg("-o").arg(format!("{}/", out.display())),
    };
    command.arg("--quiet").current_dir(&out);
    if let Some(fonts) = crate::android::fonts_dir() {
        command.env("PANPDF_FONTS", fonts);
    }
    let done = command
        .output()
        .map_err(|error| format!("{}: {error}", program.display()))?;
    let mut made: Vec<PathBuf> = std::fs::read_dir(&out)
        .map_err(|error| error.to_string())?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .collect();
    made.sort();
    if made.is_empty() {
        let said = String::from_utf8_lossy(&done.stderr).trim().to_owned();
        return Err(if said.is_empty() {
            "the tool made nothing".to_owned()
        } else {
            said
        });
    }
    Ok(made)
}
