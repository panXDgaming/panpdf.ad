use std::path::Path;

use eframe::egui;

use pdf_app::Editor;
use pdf_app::wording::{Command, Lang, Message};
use pdf_bytes::{ByteStore, SourceId};

use crate::app::name_of;
use crate::icons::Icon;
use crate::room;
use crate::window_state::{
    LeaveChoice, Leaving, Opened, Opening, Pointing, Tool, Window, set_dark,
};

const MM_PER_POINT: f64 = 25.4 / 72.0;

pub(crate) const A4: [f64; 2] = [595.28, 841.89];

const PAPER_SIZES: &[(&str, [f64; 2])] = &[
    ("A3", [841.89, 1190.55]),
    ("A4", A4),
    ("A5", [419.53, 595.28]),
    ("B4", [708.66, 1000.63]),
    ("B5", [498.9, 708.66]),
    ("Letter", [612.0, 792.0]),
    ("Legal", [612.0, 1008.0]),
    ("Tabloid", [792.0, 1224.0]),
];

pub(crate) fn open_bytes(source: ByteStore, credential: &[u8]) -> Opened {
    if pdf_edit::info::lock(&source, credential) == pdf_edit::info::Lock::Refused {
        return Opened::Locked(source);
    }
    match Editor::open_with(source, credential) {
        Ok(mut editor) => {
            editor.set_clock(crate::moment::millis);
            Opened::Document(Box::new(editor))
        }
        Err(reason) => Opened::Refused(reason),
    }
}

impl Window {
    fn file_menu(&mut self, ui: &mut egui::Ui, (idle, working): (bool, bool)) {
        let lang = self.lang;
        let say = |command| Message::Command(command).say(lang);
        let home = self.has_document() && !self.home;
        if ui
            .add_enabled(home, egui::Button::new(say(Command::Home)))
            .clicked()
        {
            self.go_home();
            ui.close();
        }
        ui.separator();
        ui.add_enabled_ui(idle, |ui| {
            ui.menu_button(say(Command::NewDocument), |ui| {
                if let Some(size) = self.page_size_menu(ui) {
                    self.new_document(size);
                    ui.close();
                }
            });
        });
        if ui
            .add_enabled(idle, egui::Button::new(say(Command::Open)))
            .clicked()
        {
            self.asking_to_open = true;
            ui.close();
        }
        ui.add_enabled_ui(self.library.len() > 1, |ui| {
            ui.menu_button(say(Command::Documents), |ui| {
                for path in self.library.clone() {
                    let open = path == self.opened;
                    let button = egui::Button::selectable(open, name_of(&path));
                    if ui.add_enabled(idle || open, button).clicked() {
                        self.open(&path);
                        ui.close();
                    }
                }
            });
        });
        ui.separator();
        let save = egui::Button::new(say(Command::Save)).shortcut_text("Ctrl+S");
        if ui.add_enabled(working, save).clicked() {
            self.save();
            ui.close();
        }
        let save_as = egui::Button::new(say(Command::SaveAs)).shortcut_text("Ctrl+Shift+S");
        if ui.add_enabled(working, save_as).clicked() {
            self.save_a_copy_as();
            ui.close();
        }
        let split = egui::Button::new(say(Command::SplitDocument));
        if ui.add_enabled(working, split).clicked() {
            self.open_the_split_panel();
            ui.close();
        }
        ui.separator();
        let pictures = egui::Button::new(say(Command::PagesAsPictures));
        if ui.add_enabled(working, pictures).clicked() {
            self.open_the_export_panel();
            ui.close();
        }
        if ui.button(say(Command::PdfFromPictures)).clicked() {
            self.choose_pictures(None);
            ui.close();
        }
        ui.separator();
        let print = egui::Button::new(say(Command::Print)).shortcut_text("Ctrl+P");
        if ui.add_enabled(working, print).clicked() {
            self.open_the_print_dialog();
            ui.close();
        }
        ui.separator();
        let properties =
            egui::Button::new(pdf_app::wording::Fact::Properties.say(lang)).shortcut_text("Ctrl+D");
        if ui.add_enabled(working, properties).clicked() {
            self.open_the_properties();
            ui.close();
        }
    }

    pub(crate) fn menu_bar(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang;
        let say = |command| Message::Command(command).say(lang);
        let idle = !self.editor.is_busy() && self.loading.is_none();
        let working = idle && self.has_document() && !self.home;
        egui::Panel::top("menu").show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button(say(Command::File), |ui| {
                    self.file_menu(ui, (idle, working));
                });
                ui.menu_button(say(Command::Edit), |ui| {
                    let undo = egui::Button::new(say(Command::Undo)).shortcut_text("Ctrl+Z");
                    if ui
                        .add_enabled(working && self.editor.can_undo(), undo)
                        .clicked()
                    {
                        self.walk_history(true);
                        ui.close();
                    }
                    let redo = egui::Button::new(say(Command::Redo)).shortcut_text("Ctrl+Y");
                    if ui
                        .add_enabled(working && self.editor.can_redo(), redo)
                        .clicked()
                    {
                        self.walk_history(false);
                        ui.close();
                    }
                    ui.separator();
                    let copy = egui::Button::new(say(Command::Copy)).shortcut_text("Ctrl+C");
                    if ui.add_enabled(working && self.selected(), copy).clicked() {
                        let ctx = ui.ctx().clone();
                        let in_text = self.pointing.editing();
                        self.copy(&ctx, in_text);
                        ui.close();
                    }
                    let cut = egui::Button::new(say(Command::Cut)).shortcut_text("Ctrl+X");
                    if ui.add_enabled(working && self.selected(), cut).clicked() {
                        let ctx = ui.ctx().clone();
                        let in_text = self.pointing.editing();
                        self.cut(&ctx, in_text);
                        ui.close();
                    }
                    let holding = self.clipboard.is_some();
                    let paste = egui::Button::new(say(Command::Paste)).shortcut_text("Ctrl+V");
                    if ui.add_enabled(working && holding, paste).clicked() {
                        let ctx = ui.ctx().clone();
                        self.paste_the_clipboard(&ctx, false);
                        ui.close();
                    }
                    let in_place =
                        egui::Button::new(say(Command::PasteInPlace)).shortcut_text("Ctrl+Shift+V");
                    if ui.add_enabled(working && holding, in_place).clicked() {
                        let ctx = ui.ctx().clone();
                        self.paste_the_clipboard(&ctx, true);
                        ui.close();
                    }
                    let delete = egui::Button::new(say(Command::Delete)).shortcut_text("Del");
                    if ui.add_enabled(working && self.selected(), delete).clicked() {
                        self.delete();
                        ui.close();
                    }
                    ui.separator();
                    let ordering = working && self.can_order();
                    for (command, order) in [
                        (Command::BringToFront, pdf_edit::Stacking::ToFront),
                        (Command::BringForward, pdf_edit::Stacking::Forward),
                        (Command::SendBackward, pdf_edit::Stacking::Backward),
                        (Command::SendToBack, pdf_edit::Stacking::ToBack),
                    ] {
                        if ui
                            .add_enabled(ordering, egui::Button::new(say(command)))
                            .clicked()
                        {
                            self.put_in_order(order);
                            ui.close();
                        }
                    }
                    ui.separator();
                    let find = egui::Button::new(say(Command::Find)).shortcut_text("Ctrl+F");
                    if ui.add_enabled(working, find).clicked() {
                        self.open_the_find_bar();
                        ui.close();
                    }
                    if self.editor.editing_restricted() {
                        ui.separator();
                        let allow = egui::Button::new(say(Command::AllowEditing));
                        if ui.add_enabled(working, allow).clicked() {
                            self.restriction_answered = false;
                            ui.close();
                        }
                    }
                });
                ui.menu_button(say(Command::Insert), |ui| self.insert_menu(ui, working));
                ui.menu_button(say(Command::Page), |ui| self.page_menu(ui, working));
                ui.menu_button(say(Command::Tools), |ui| self.tools_menu(ui, working));
                ui.menu_button(say(Command::View), |ui| self.view_menu(ui, working));
                #[cfg(not(target_arch = "wasm32"))]
                ui.menu_button(say(Command::Help), |ui| self.help_menu(ui));
            });
        });
    }

    fn insert_menu(&mut self, ui: &mut egui::Ui, working: bool) {
        let lang = self.lang;
        let say = |command| Message::Command(command).say(lang);
        ui.add_enabled_ui(working, |ui| {
            if ui.button(say(Command::InsertText)).clicked() {
                self.pictures.clear();
                self.tool = Tool::Text;
                ui.close();
            }
            if ui.button(say(Command::InsertPictures)).clicked() {
                self.choosing_for = crate::page_actions::Choosing::Picture;
                self.asking_to_open = true;
                ui.close();
            }
            for (command, tool) in [
                (Command::Shape, Tool::Shape),
                (Command::Pen, Tool::Pen),
                (Command::Highlighter, Tool::Highlighter),
            ] {
                if ui.button(say(command)).clicked() {
                    self.take_up(tool);
                    ui.close();
                }
            }
            if ui.button(say(Command::Link)).clicked() {
                self.take_up_the_link_tool();
                ui.close();
            }
            if ui.button(say(Command::InsertField)).clicked() {
                self.take_up_the_form_tool();
                ui.close();
            }
            ui.separator();
            self.pages_in_menu(ui);
        });
    }

    fn tools_menu(&mut self, ui: &mut egui::Ui, working: bool) {
        let lang = self.lang;
        let say = |command| Message::Command(command).say(lang);
        ui.add_enabled_ui(working, |ui| {
            #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
            {
                if ui.button(say(Command::AiAssistant)).clicked() {
                    let now = ui.input(|input| input.time);
                    self.open_ai_panel(now);
                    ui.close();
                }
                if ui.button(say(Command::ConnectAgents)).clicked() {
                    self.open_the_agents_window();
                    ui.close();
                }
                ui.separator();
            }
            if ui.button(say(Command::RecognizeText)).clicked() {
                self.open_the_ocr_panel();
                ui.close();
            }
            ui.separator();
            for (command, kind) in [
                (
                    Command::StampPageNumbers,
                    crate::stamp_tool::StampKind::PageNumbers,
                ),
                (
                    Command::StampHeaderFooter,
                    crate::stamp_tool::StampKind::HeaderFooter,
                ),
                (
                    Command::StampWatermark,
                    crate::stamp_tool::StampKind::Watermark,
                ),
            ] {
                if ui.button(say(command)).clicked() {
                    self.open_the_stamp_panel(kind);
                    ui.close();
                }
            }
        });
    }

    fn view_menu(&mut self, ui: &mut egui::Ui, working: bool) {
        let lang = self.lang;
        let say = |command| Message::Command(command).say(lang);
        let closer = egui::Button::new(say(Command::ZoomIn)).shortcut_text("Ctrl++");
        if ui.add_enabled(working, closer).clicked() {
            self.zoom_by(true, None);
        }
        let further = egui::Button::new(say(Command::ZoomOut)).shortcut_text("Ctrl+-");
        if ui.add_enabled(working, further).clicked() {
            self.zoom_by(false, None);
        }
        ui.separator();
        let mut pages = !self.pages_folded;
        if ui.checkbox(&mut pages, say(Command::Pages)).changed() {
            let now = ui.input(|input| input.time);
            self.fold_the_pages(!pages, now);
        }
        let _ = ui.checkbox(&mut self.show_contents, say(Command::Contents));
        ui.menu_button(say(Command::Frames), |ui| {
            let all = egui::Button::new(say(Command::ShowFrames))
                .selected(self.show_frames)
                .shortcut_text("F2");
            if ui.add(all).clicked() {
                self.show_frames = !self.show_frames;
            }
            ui.separator();
            ui.add_enabled_ui(self.show_frames, |ui| {
                let _ = ui.checkbox(&mut self.framed.text, say(Command::FramesOfText));
                let _ = ui.checkbox(&mut self.framed.pictures, say(Command::FramesOfPictures));
                let _ = ui.checkbox(&mut self.framed.drawings, say(Command::FramesOfDrawings));
            });
        });
        if ui
            .checkbox(&mut self.dark, say(Command::DarkMode))
            .changed()
        {
            set_dark(ui.ctx(), self.dark);
        }
        let _ = ui.checkbox(&mut self.show_speed, say(Command::ShowDrawingSpeed));
        ui.menu_button(say(Command::Language), |ui| {
            for (language, named) in language_rows() {
                let _ = ui.radio_value(&mut self.lang, language, named);
            }
        });
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn help_menu(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang;
        let say = |command| Message::Command(command).say(lang);
        if ui.button(say(Command::ReportAProblem)).clicked() {
            self.open_out(&crate::reporting::report_link());
            ui.close();
        }
        let folder = crate::reporting::log_folder();
        let show = egui::Button::new(say(Command::ShowTheLog));
        if ui.add_enabled(folder.is_some(), show).clicked() {
            if let Some(folder) = folder {
                self.open_out(&folder.to_string_lossy());
            }
            ui.close();
        }
    }

    pub(crate) fn page_size_menu(&self, ui: &mut egui::Ui) -> Option<[f64; 2]> {
        let say = |command| Message::Command(command).say(self.lang);
        let turned = |[width, height]: [f64; 2]| {
            if self.landscape == (width > height) {
                [width, height]
            } else {
                [height, width]
            }
        };
        let points = |[width, height]: [f64; 2]| {
            format!(
                "{:.0} × {:.0} mm",
                width * MM_PER_POINT,
                height * MM_PER_POINT
            )
        };
        let mut chosen = None;
        let showing = self.has_document() && !self.home;
        if let Some(geometry) = showing.then(|| self.editor.geometry(self.focus)).flatten() {
            let [x0, y0, x1, y1] = geometry.media_box;
            let size = [(x1 - x0).abs(), (y1 - y0).abs()];
            let label = format!("{} ({})", say(Command::SameSizeAsThisPage), points(size));
            if ui.button(label).clicked() {
                chosen = Some(size);
            }
            ui.separator();
        }
        for (name, size) in PAPER_SIZES {
            let size = turned(*size);
            if ui.button(format!("{name} ({})", points(size))).clicked() {
                chosen = Some(size);
            }
        }
        chosen
    }

    pub(crate) fn selected(&self) -> bool {
        matches!(
            self.pointing,
            Pointing::Text { caret, .. } if caret.at != caret.anchor
        ) || matches!(self.pointing, Pointing::Block { .. })
            || (self.tool == Tool::Form && self.chosen_fields.is_some())
            || self.pointing.object_on(self.focus).is_some()
    }

    pub(crate) fn toolbar(&mut self, ui: &mut egui::Ui) {
        let strip = self.tools_in_a_strip(ui.ctx());
        let mut strip_slots = Vec::new();
        egui::Panel::top("toolbar").show(ui, |ui| {
            ui.add_space(3.0);
            ui.horizontal(|ui| {
                let room = ui.available_width();
                let idle = !self.editor.is_busy() && self.loading.is_none();
                let lang = self.lang;
                let mut slots = self.bar_slots(ui, idle, lang);
                if strip {
                    let (tools, rest): (Vec<Slot>, Vec<Slot>) =
                        slots.into_iter().partition(Slot::belongs_in_the_strip);
                    strip_slots = tools;
                    slots = rest;
                }

                let bar = room::Bar {
                    buttons: slots.iter().filter(|slot| slot.is_button()).count(),
                    rules: slots.iter().filter(|slot| slot.is_rule()).count(),
                    choices: self.toolbar_choices,
                    readouts: slots.iter().map(Slot::readout_width).sum(),
                    slack: self.toolbar_slack,
                };
                let labels = room::labels_fit(room, &bar, !self.toolbar_compact);
                self.toolbar_compact = !labels;

                let mut pieces: Vec<room::Piece> =
                    slots.iter().map(|slot| slot.piece(labels)).collect();
                pieces.push(room::Piece {
                    width: self.toolbar_choices + room::EDGE + self.toolbar_slack,
                    rank: 0,
                });
                let cut = room::shed_above(room, &pieces, room::OVERFLOW_WIDTH);

                let start = ui.cursor().min.x;
                ui.add_space(4.0);
                let mut pressed = None;
                let mut choices_width = 0.0;
                let mut drawn = 0.0;
                let mut anything_yet = false;
                for slot in slots.iter().filter(|slot| slot.side == Side::Left) {
                    if let Some(command) =
                        self.draw_slot(ui, slot, cut, &mut anything_yet, &mut drawn)
                    {
                        pressed = Some(command);
                    }
                }
                if self.tool_has_choices() && !strip {
                    toolbar_separator(ui);
                    let before = ui.cursor().min.x;
                    self.tool_choices(ui);
                    choices_width = ui.cursor().min.x - before;
                }
                self.toolbar_choices = choices_width;

                let left_end = ui.cursor().min.x;
                let mut right_width = 0.0;
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if self.editor.is_busy() || self.loading.is_some() {
                        ui.spinner();
                    }
                    if cut != room::ALL
                        && let Some(command) =
                            Self::overflow_menu(ui, &slots, (cut, lang), self.touched)
                    {
                        pressed = Some(command);
                    }
                    let mut yet = true;
                    for slot in slots.iter().filter(|slot| slot.side == Side::Right) {
                        if let Some(command) = self.draw_slot(ui, slot, cut, &mut yet, &mut drawn) {
                            pressed = Some(command);
                        }
                    }
                    right_width = ui.min_rect().width();
                });
                let measured = (left_end - start - choices_width) + right_width;
                self.toolbar_slack = (measured - drawn).clamp(0.0, 80.0);
                if let Some(command) = pressed {
                    let ctx = ui.ctx().clone();
                    self.run_from_the_bar(&ctx, command);
                }
                self.let_go_of_text_the_tool_may_not_hold();
            });
            if self.finding.is_some() {
                self.find_bar(ui);
            }
            ui.add_space(3.0);
        });
        if !strip_slots.is_empty() {
            self.tool_strip(ui, &strip_slots);
        }
    }

    pub(crate) fn tools_in_a_strip(&self, ctx: &egui::Context) -> bool {
        (self.touched || cfg!(target_os = "android"))
            && !self.viewing
            && self.has_document()
            && ctx.content_rect().width() < STRIP_BELOW
    }

    fn tool_strip(&mut self, ui: &mut egui::Ui, slots: &[Slot]) {
        let mut pressed = None;
        egui::Panel::bottom("tool-strip").show(ui, |ui| {
            if self.tool_has_choices() {
                let mut row = egui::ScrollArea::horizontal()
                    .id_salt("tool-choices-row")
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden);
                if let Some(to) = self.choices_slide_to.take() {
                    row = row.horizontal_scroll_offset(to);
                }
                let shown = row.show(ui, |ui| {
                    ui.horizontal(|ui| self.tool_choices(ui));
                });
                Self::more_this_way(
                    ui,
                    shown.inner_rect,
                    (shown.content_size.x, shown.state.offset.x),
                    &mut self.choices_slide_to,
                );
                ui.separator();
            }
            egui::ScrollArea::horizontal()
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        for slot in slots {
                            if let What::Button {
                                icon,
                                command,
                                enabled,
                                on,
                            } = slot.what
                                && self.tool_button_as(ui, icon, command, enabled, on, true)
                            {
                                pressed = Some(command);
                            }
                        }
                    });
                });
        });
        if let Some(command) = pressed {
            let ctx = ui.ctx().clone();
            self.run_from_the_bar(&ctx, command);
        }
    }

    fn bar_slots(&self, ui: &egui::Ui, idle: bool, lang: Lang) -> Vec<Slot> {
        let slots = self.every_bar_slot(ui, idle, lang);
        #[cfg(target_arch = "wasm32")]
        let slots = slots
            .into_iter()
            .filter(|slot| {
                !matches!(
                    slot.what,
                    What::Button {
                        command: Command::Home | Command::NewDocument,
                        ..
                    }
                )
            })
            .collect();
        slots
    }

    #[expect(
        clippy::too_many_lines,
        reason = "one list of every slot in reading order: split, the plan it is would be split too"
    )]
    fn every_bar_slot(&self, ui: &egui::Ui, idle: bool, lang: Lang) -> Vec<Slot> {
        let button = |icon, command, enabled, on, rank, side| Slot {
            side,
            rank,
            what: What::Button {
                icon,
                command,
                enabled,
                on,
            },
        };
        let rule = |rank, side| Slot {
            side,
            rank,
            what: What::Rule,
        };
        let mut slots = vec![
            button(
                Icon::Edit,
                Command::EditMode,
                idle && self.has_document(),
                !self.viewing,
                0,
                Side::Left,
            ),
            rule(0, Side::Left),
            button(
                Icon::Home,
                Command::Home,
                idle,
                false,
                HOME_RANK,
                Side::Left,
            ),
            button(
                Icon::NewDocument,
                Command::NewDocument,
                idle,
                false,
                HOME_RANK,
                Side::Left,
            ),
            button(
                Icon::Open,
                Command::Open,
                idle,
                false,
                OPEN_RANK,
                Side::Left,
            ),
            #[cfg(not(target_arch = "wasm32"))]
            button(
                Icon::Save,
                Command::Save,
                idle,
                false,
                SAVE_RANK,
                Side::Left,
            ),
            #[cfg(target_arch = "wasm32")]
            button(
                Icon::Export,
                Command::Export,
                idle,
                false,
                SAVE_RANK,
                Side::Left,
            ),
        ];
        #[cfg(target_os = "android")]
        {
            slots.retain(|slot| {
                !matches!(
                    slot.what,
                    What::Button {
                        command: Command::Save,
                        ..
                    }
                )
            });
            for (icon, command) in [(Icon::Share, Command::Share), (Icon::Save, Command::Save)] {
                slots.push(button(
                    icon,
                    command,
                    idle && self.has_document(),
                    false,
                    ALWAYS_RANK,
                    Side::Right,
                ));
            }
        }
        #[cfg(target_arch = "wasm32")]
        if self.touched {
            for slot in &mut slots {
                if matches!(
                    slot.what,
                    What::Button {
                        command: Command::Export,
                        ..
                    }
                ) {
                    slot.side = Side::Right;
                    slot.rank = ALWAYS_RANK;
                }
            }
        }
        slots.push(button(
            Icon::Find,
            Command::Find,
            self.has_document(),
            self.finding.is_some(),
            if self.touched || cfg!(target_os = "android") {
                ALWAYS_RANK
            } else {
                DELETE_RANK
            },
            Side::Right,
        ));
        #[cfg(target_arch = "wasm32")]
        if self.editor.editing_restricted() {
            slots.push(button(
                Icon::Unlock,
                Command::AllowEditing,
                idle,
                false,
                SAVE_RANK,
                Side::Left,
            ));
        }
        slots.extend([
            rule(HISTORY_RANK, Side::Left),
            button(
                Icon::Undo,
                Command::Undo,
                idle && self.editor.can_undo(),
                false,
                HISTORY_RANK,
                Side::Left,
            ),
            button(
                Icon::Redo,
                Command::Redo,
                idle && self.editor.can_redo(),
                false,
                HISTORY_RANK,
                Side::Left,
            ),
        ]);
        #[cfg(target_os = "android")]
        if !self.viewing {
            slots.push(button(
                Icon::Scan,
                Command::ScanPages,
                idle && self.has_document(),
                false,
                HISTORY_RANK,
                Side::Left,
            ));
        }
        slots.push(rule(0, Side::Left));
        for (icon, command, tool) in TOOLS {
            if self.viewing && tool != Tool::Select {
                continue;
            }
            slots.push(button(
                icon,
                command,
                if tool == Tool::Select { true } else { idle },
                self.tool == tool,
                tool_rank(tool),
                Side::Left,
            ));
            #[cfg(target_os = "android")]
            if tool == Tool::Select {
                slots.push(button(
                    Icon::Ocr,
                    Command::Ocr,
                    idle && self.has_document(),
                    false,
                    if self.viewing { 0 } else { RARE_TOOL_RANK },
                    Side::Left,
                ));
            }
            #[cfg(target_arch = "wasm32")]
            if self.viewing && tool == Tool::Select {
                slots.push(button(
                    Icon::Ocr,
                    Command::Ocr,
                    idle,
                    self.web_ocr.is_some(),
                    0,
                    Side::Left,
                ));
            }
            #[cfg(target_arch = "wasm32")]
            if tool == Tool::Link {
                slots.push(button(
                    Icon::Watermark,
                    Command::StampWatermark,
                    idle,
                    self.stamp_draft.is_some(),
                    RARE_TOOL_RANK,
                    Side::Left,
                ));
                slots.push(button(
                    Icon::Ocr,
                    Command::Ocr,
                    idle,
                    self.web_ocr.is_some(),
                    RARE_TOOL_RANK,
                    Side::Left,
                ));
            }
        }
        if !self.viewing {
            slots.push(button(
                Icon::Delete,
                Command::Delete,
                idle && self.selected(),
                false,
                DELETE_RANK,
                Side::Left,
            ));
        }
        slots.extend(self.view_slots(ui, lang));
        slots
    }

    fn view_slots(&self, ui: &egui::Ui, lang: Lang) -> Vec<Slot> {
        let button = |icon, command, enabled, rank| Slot {
            side: Side::Right,
            rank,
            what: What::Button {
                icon,
                command,
                enabled,
                on: false,
            },
        };
        let rule = |rank| Slot {
            side: Side::Right,
            rank,
            what: What::Rule,
        };
        let readout = |text: String, rank| Slot {
            side: Side::Right,
            rank,
            what: What::Readout {
                width: text_width(ui, &text),
                text,
            },
        };
        let mut slots = Vec::new();
        slots.push(button(Icon::Theme, Command::Theme, true, THEME_RANK));
        slots.push(rule(THEME_RANK));
        slots.push(button(Icon::ZoomIn, Command::ZoomIn, true, ZOOM_RANK));
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a zoom is between 25% and 400%"
        )]
        let percent = (self.zoom * 100.0).round() as u32;
        slots.push(readout(Message::ZoomPercent(percent).say(lang), ZOOM_RANK));
        slots.push(button(Icon::ZoomOut, Command::ZoomOut, true, ZOOM_RANK));
        slots.push(rule(PAGING_RANK));
        let pages = self.editor.page_count();
        slots.push(button(
            Icon::Next,
            Command::NextPage,
            self.focus + 1 < pages,
            PAGING_RANK,
        ));
        slots.push(readout(
            Message::PageOf {
                page: self.focus + 1,
                count: pages,
            }
            .say(lang),
            PAGING_RANK,
        ));
        slots.push(button(
            Icon::Previous,
            Command::PreviousPage,
            self.focus > 0,
            PAGING_RANK,
        ));
        slots
    }

    fn draw_slot(
        &self,
        ui: &mut egui::Ui,
        slot: &Slot,
        cut: u8,
        anything_yet: &mut bool,
        drawn: &mut f32,
    ) -> Option<Command> {
        if slot.rank >= cut {
            return None;
        }
        *drawn += slot.piece(!self.toolbar_compact).width;
        match &slot.what {
            What::Rule => {
                if *anything_yet {
                    toolbar_separator(ui);
                    *anything_yet = false;
                }
                None
            }
            What::Readout { text, .. } => {
                *anything_yet = true;
                ui.label(text);
                None
            }
            What::Button {
                icon,
                command,
                enabled,
                on,
            } => {
                *anything_yet = true;
                self.tool_button(ui, *icon, *command, *enabled, *on)
                    .then_some(*command)
            }
        }
    }

    fn overflow_menu(
        ui: &mut egui::Ui,
        slots: &[Slot],
        (cut, lang): (u8, Lang),
        touched: bool,
    ) -> Option<Command> {
        let name = Message::Command(Command::MoreForPage).say(lang);
        let opened = crate::format::icon_button(ui, Icon::More, &name, false, true);
        let mut pressed = None;
        egui::Popup::menu(&opened).show(|ui| {
            ui.set_min_width(180.0);
            if touched {
                ui.spacing_mut().button_padding = egui::vec2(12.0, 10.0);
            }
            for slot in slots {
                if slot.rank < cut {
                    continue;
                }
                let What::Button {
                    command, enabled, ..
                } = slot.what
                else {
                    continue;
                };
                let label = Message::Command(command).say(lang);
                if ui.add_enabled(enabled, egui::Button::new(label)).clicked() {
                    pressed = Some(command);
                    ui.close();
                }
            }
        });
        pressed
    }

    fn run_from_the_bar(&mut self, ctx: &egui::Context, command: Command) {
        match command {
            Command::Home => self.go_home(),
            Command::NewDocument => self.new_document(A4),
            Command::Open => self.asking_to_open = true,
            #[cfg(target_os = "android")]
            Command::ScanPages => crate::android::scan(true),
            #[cfg(target_os = "android")]
            Command::Share => self.share(),
            #[cfg(target_os = "android")]
            Command::RecognizeText | Command::Ocr => self.open_the_ocr_panel(),
            Command::Save => {
                self.save();
            }
            #[cfg(target_arch = "wasm32")]
            Command::Export => self.choosing_an_export = true,
            #[cfg(target_arch = "wasm32")]
            Command::AllowEditing => self.restriction_answered = false,
            #[cfg(target_arch = "wasm32")]
            Command::StampWatermark => {
                self.open_the_stamp_panel(crate::stamp_tool::StampKind::Watermark);
            }
            #[cfg(target_arch = "wasm32")]
            Command::Ocr => self.open_the_web_ocr_panel(),
            Command::Undo => {
                self.walk_history(true);
            }
            Command::Redo => {
                self.walk_history(false);
            }
            Command::Delete => {
                if !self.remove_the_chosen_fields() {
                    self.delete();
                }
            }
            Command::EditMode => {
                self.viewing = !self.viewing;
                self.reading = None;
                if self.viewing {
                    self.tool = Tool::Select;
                    self.point_at(Pointing::Nothing);
                }
            }
            Command::Select => {
                self.tool = Tool::Select;
                self.pictures.clear();
                self.ink = None;
            }
            Command::Text => {
                self.pictures.clear();
                self.tool = Tool::Text;
            }
            Command::Pen => self.take_up(Tool::Pen),
            Command::Highlighter => self.take_up(Tool::Highlighter),
            Command::Shape => self.take_up(Tool::Shape),
            Command::Form => self.take_up_the_form_tool(),
            Command::Link => self.take_up_the_link_tool(),
            Command::Picture => {
                self.choosing_for = crate::page_actions::Choosing::Picture;
                self.asking_to_open = true;
            }
            Command::Theme => {
                self.dark = !self.dark;
                set_dark(ctx, self.dark);
            }
            Command::Find => {
                if self.finding.is_some() {
                    self.close_the_find_bar();
                } else {
                    self.open_the_find_bar();
                }
            }
            Command::ZoomIn => self.zoom_by(true, None),
            Command::ZoomOut => self.zoom_by(false, None),
            Command::NextPage => self.goto(self.focus + 1),
            Command::PreviousPage => self.goto(self.focus.saturating_sub(1)),
            _ => {}
        }
    }

    fn let_go_of_text_the_tool_may_not_hold(&mut self) {
        if self.tool.edits_text() {
            return;
        }
        let kept = self.pointing;
        self.point_at(kept);
        self.drop_the_text_draft();
    }

    fn tool_has_choices(&self) -> bool {
        self.tool.is_drawing() || matches!(self.tool, Tool::Form | Tool::Link)
    }

    fn tool_choices(&mut self, ui: &mut egui::Ui) {
        match self.tool {
            Tool::Form => self.form_tool_choices(ui),
            Tool::Link => self.link_tool_choices(ui),
            Tool::Shape => {
                self.shape_choices(ui);
                crate::format::rule(ui);
                self.pen_choices(ui);
            }
            Tool::Pen | Tool::Highlighter => self.pen_choices(ui),
            Tool::Select | Tool::Text | Tool::Picture => {}
        }
    }

    pub(crate) fn tool_hint(&self) -> Option<String> {
        match self.tool {
            Tool::Form => self.form_tool_hint(),
            Tool::Link => self.link_tool_hint(),
            Tool::Select
            | Tool::Text
            | Tool::Pen
            | Tool::Highlighter
            | Tool::Shape
            | Tool::Picture => None,
        }
    }

    fn tool_button(
        &self,
        ui: &mut egui::Ui,
        icon: Icon,
        command: Command,
        enabled: bool,
        on: bool,
    ) -> bool {
        self.tool_button_as(ui, icon, command, enabled, on, !self.toolbar_compact)
    }

    fn tool_button_as(
        &self,
        ui: &mut egui::Ui,
        icon: Icon,
        command: Command,
        enabled: bool,
        on: bool,
        labelled: bool,
    ) -> bool {
        let name = Message::Command(command).say(self.lang);
        let width = room::tool_width(labelled);
        let size = egui::vec2(width, room::TOOL_HEIGHT);
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
        let response = response.on_hover_text(&name);
        let visuals = ui.visuals();
        let colour = if !enabled {
            visuals.weak_text_color()
        } else if on {
            visuals.selection.stroke.color
        } else {
            visuals.text_color()
        };
        if ui.is_rect_visible(rect) {
            if on {
                ui.painter()
                    .rect_filled(rect, 4.0, visuals.selection.bg_fill.gamma_multiply(0.35));
            } else if enabled && response.hovered() {
                ui.painter()
                    .rect_filled(rect, 4.0, visuals.widgets.hovered.bg_fill);
            }
            let top = if labelled {
                rect.top() + 4.0
            } else {
                rect.center().y - room::ICON_SIDE / 2.0
            };
            let glyph = egui::Rect::from_min_size(
                egui::pos2(rect.center().x - room::ICON_SIDE / 2.0, top),
                egui::vec2(room::ICON_SIDE, room::ICON_SIDE),
            );
            icon.draw_tinted(ui.painter(), glyph, colour, enabled);
            if cfg!(target_os = "android") && command == Command::Save && enabled && self.unsaved()
            {
                ui.painter().circle_filled(
                    glyph.right_top() + egui::vec2(1.0, 1.0),
                    3.5,
                    egui::Color32::from_rgb(234, 120, 20),
                );
            }
            if !labelled {
                return enabled && response.clicked();
            }
            let label = match command {
                Command::Find => "Find",
                _ => name.as_str(),
            };
            ui.painter().text(
                egui::pos2(rect.center().x, rect.bottom() - 3.0),
                egui::Align2::CENTER_BOTTOM,
                label,
                egui::FontId::proportional(10.0),
                colour,
            );
        }
        enabled && response.clicked()
    }

    pub(crate) fn open(&mut self, path: &Path) {
        self.open_at(path, 0);
    }

    pub(crate) fn open_at(&mut self, path: &Path, page: usize) {
        if self.editor.is_busy() || self.loading.is_some() || self.unlocking.is_some() {
            return;
        }
        if *path == self.opened {
            self.home = false;
            self.goto(page.min(self.editor.page_count().saturating_sub(1)));
            return;
        }
        if self.unsaved() {
            self.leaving = Some(Leaving::Open(path.to_path_buf(), page));
            return;
        }
        self.open_now(path, page);
    }

    fn open_now(&mut self, path: &Path, page: usize) {
        #[cfg(not(target_arch = "wasm32"))]
        crate::reporting::say(
            pdf_app::trouble::Kind::Document,
            &format!("opening {}", path.display()),
        );
        self.remember_here();
        let wanted = path.to_path_buf();
        let reading = wanted.clone();
        let opening = Message::Opening(name_of(&wanted));
        self.editor.say(opening);
        self.loading = Some(Opening {
            path: wanted,
            page,
            changed_protection: false,
            tried_a_password: false,
            handle: crate::task::spawn(move || match std::fs::read(&reading) {
                Ok(bytes) => open_bytes(ByteStore::owning(SourceId::next_document(), bytes), b""),
                Err(error) => Opened::Refused(format!("{}: {error}", reading.display())),
            }),
        });
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn export_choice(&mut self, ctx: &egui::Context) {
        if !self.choosing_an_export {
            return;
        }
        let (mut close, mut pdf, mut pictures) = (false, false, false);
        let modal = egui::Modal::new(egui::Id::new("export-choice")).show(ctx, |ui| {
            ui.set_width(crate::side_panel::box_width(ui.ctx(), 380.0));
            ui.heading("Export");
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("Choose the kind of file to download.")
                    .color(ui.visuals().weak_text_color()),
            );
            ui.add_space(12.0);
            let wide = ui.available_width();
            pdf = crate::hub::action_tile(
                ui,
                wide,
                Icon::Document,
                "PDF document (.pdf)",
                "The file with your edits, everything else as it was",
                true,
            );
            ui.add_space(8.0);
            pictures = crate::hub::action_tile(
                ui,
                wide,
                Icon::Picture,
                "Pictures (.png)",
                "One picture for each page, at the size you choose",
                true,
            );
            ui.add_space(12.0);
            if ui
                .button(Message::Home(pdf_app::wording::Home::Cancel).say(self.lang))
                .clicked()
            {
                close = true;
            }
        });
        if modal.should_close() {
            close = true;
        }
        if pdf {
            self.choosing_an_export = false;
            self.save();
        } else if pictures {
            self.choosing_an_export = false;
            self.open_the_export_panel();
        } else if close {
            self.choosing_an_export = false;
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn take_from_the_page(&mut self, ctx: &egui::Context) {
        crate::web_files::remember(ctx);
        ctx.input(|input| {
            for event in &input.events {
                if let egui::Event::Text(text) | egui::Event::Paste(text) = event {
                    crate::web_files::want_text(text);
                }
            }
        });
        if crate::web_files::fonts_grew() && self.input.draft().is_some() && !self.input.pending() {
            self.retry_draft();
        }
        while let Some((pick, files)) = crate::web_files::take_picked() {
            match pick {
                crate::web_files::Pick::Picture => self.pictures_given_to_place(files),
                crate::web_files::Pick::Pages { before } => {
                    if let Some((name, bytes)) = files.into_iter().next() {
                        self.insert_pages_given(name, std::sync::Arc::from(bytes), before);
                    }
                }
                crate::web_files::Pick::PicturesIn { before } => self.pictures_given(files, before),
            }
        }
        let unsaved = self.has_document() && self.unsaved();
        crate::web_files::mark_unsaved(unsaved);
        crate::web_files::tell_the_page(
            self.editor.opened().map(|(id, _)| id),
            self.editor.epoch(),
            unsaved,
            self.live
                .as_ref()
                .map_or(0, crate::live_typing::LiveTyping::fingerprint),
            &self.opened,
        );
        if self.editor.is_busy()
            || self.loading.is_some()
            || self.unlocking.is_some()
            || self.leaving.is_some()
            || !pdf_cli::fonts_held()
        {
            return;
        }
        let Some((name, bytes)) = crate::web_files::take() else {
            return;
        };
        if self.has_document() && self.unsaved() {
            self.leaving = Some(Leaving::OpenGiven(name, bytes));
            return;
        }
        self.open_given_now(&name, bytes, ctx);
    }

    #[cfg(target_arch = "wasm32")]
    fn open_given_now(&mut self, name: &str, bytes: Vec<u8>, ctx: &egui::Context) {
        let wanted = std::path::PathBuf::from(name);
        self.editor.say(Message::Opening(name_of(&wanted)));
        self.loading = Some(Opening {
            path: wanted,
            page: 0,
            changed_protection: false,
            tried_a_password: false,
            handle: crate::task::spawn(move || {
                open_bytes(ByteStore::owning(SourceId::next_document(), bytes), b"")
            }),
        });
        ctx.request_repaint();
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn save(&mut self) -> bool {
        if self.input.pending() || self.input.draft().is_some() {
            self.editor.say(Message::ResolveDraftBeforeSaving);
            return false;
        }
        let name = self.opened.file_name().map_or_else(
            || "document.pdf".to_owned(),
            |name| name.to_string_lossy().into_owned(),
        );
        crate::web_files::seed_random();
        match self.editor.export() {
            Ok(export) => match crate::web_files::download(&name, &export.bytes) {
                Ok(()) => {
                    self.saved_digest = Some(pdf_content::sha256_hex(&export.bytes));
                    self.saved_epoch = self.editor.epoch();
                    self.protection_changed = false;
                    crate::web_files::saved(&name);
                    self.editor.say(Message::SavedTo {
                        name,
                        bytes: export.bytes.len() as u64,
                    });
                    true
                }
                Err(why) => {
                    self.editor.say(Message::CouldNotSave { name, why });
                    false
                }
            },
            Err(error) => {
                self.editor.say(Message::CouldNotSave {
                    name,
                    why: error.to_string(),
                });
                false
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn title_for_the_chat(&self, path: &std::path::Path) -> String {
        path.file_name().map_or_else(
            || pdf_app::wording::Home::Untitled.say(self.lang),
            |name| name.to_string_lossy().into_owned(),
        )
    }

    fn take_the_document(
        &mut self,
        editor: pdf_app::Editor,
        path: std::path::PathBuf,
        page: usize,
    ) {
        self.editor = editor;
        self.saved_epoch = self.editor.epoch();
        self.saved_digest = None;
        self.protection_changed = false;
        self.untitled = path.as_os_str().is_empty();
        self.destination = if self.untitled {
            std::path::PathBuf::new()
        } else {
            crate::save_file::unused_copy(&path)
        };
        self.title = if self.untitled {
            pdf_app::wording::Home::Untitled.say(self.lang)
        } else {
            path.display().to_string()
        };
        let scanned = std::mem::take(&mut self.scan_arriving);
        if scanned {
            self.untitled = true;
            self.destination.clone_from(&path);
            self.title = path
                .file_stem()
                .map_or_else(String::new, |stem| stem.to_string_lossy().into_owned());
            self.saved_epoch = u64::MAX;
        }
        #[cfg(not(target_arch = "wasm32"))]
        self.ai
            .document_arrived(self.title_for_the_chat(&path), place_of(&path));
        #[cfg(target_arch = "wasm32")]
        if crate::web_files::was_restored(&path) {
            self.saved_epoch = u64::MAX;
        }
        self.opened = path;
        self.resume = None;
        self.fit_on_open = true;
        self.input = pdf_app::draft::Input::default();
        self.viewing = !self.untitled || scanned;
        self.reading = None;
        self.restriction_answered = false;
        self.point_at(Pointing::Nothing);
        self.drag = None;
        self.landing = None;
        for (id, held, slot) in self.tiles.clear() {
            self.retire(id, held, slot);
        }
        self.scenes.clear();
        self.thumbs.clear();
        self.failed.clear();
        self.chosen_pages.clear();
        self.focus = 0;
        self.wanted_offset = Some(egui::Vec2::ZERO);
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(folder) = self.opened.parent() {
            self.library = crate::hub::pdfs_in(folder);
        }
        let last = self.editor.page_count().saturating_sub(1);
        if page > 0 {
            self.goto(page.min(last));
            self.focus = page.min(last);
        }
        self.home = false;
        self.remember_here();
        let returned = pdf_heap::give_back();
        #[cfg(not(target_arch = "wasm32"))]
        crate::reporting::say(
            pdf_app::trouble::Kind::Session,
            if returned {
                "memory: returned to the system after letting a document go"
            } else {
                "memory: nothing was returned after letting a document go"
            },
        );
        #[cfg(target_arch = "wasm32")]
        let _ = returned;
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn read_the_new_document_key(&mut self, _ctx: &egui::Context) {}

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn read_the_new_document_key(&mut self, ctx: &egui::Context) {
        if self.leaving.is_none()
            && self.loading.is_none()
            && self.chooser.is_none()
            && ctx.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::N))
        {
            self.new_document(A4);
        }
    }

    pub(crate) fn let_the_document_go(&mut self) {
        if self.editor.is_busy() || self.loading.is_some() {
            return;
        }
        if self.unsaved() {
            self.leaving = Some(Leaving::LetGo);
            return;
        }
        self.drop_the_document();
    }

    fn drop_the_document(&mut self) {
        match Editor::blank(A4) {
            Ok(editor) => {
                self.take_the_document(editor, std::path::PathBuf::new(), 0);
                self.untitled = false;
                self.title = String::new();
                self.home = true;
                self.editor
                    .say(Message::Home(pdf_app::wording::Home::DocumentLetGo));
            }
            Err(reason) => self.editor.say(Message::Plain(reason)),
        }
    }

    pub(crate) fn new_document(&mut self, size: [f64; 2]) {
        if self.editor.is_busy() || self.loading.is_some() {
            return;
        }
        if self.unsaved() {
            self.leaving = Some(Leaving::New(size));
            return;
        }
        self.start_a_new_document(size);
    }

    fn start_a_new_document(&mut self, size: [f64; 2]) {
        match Editor::blank(size) {
            Ok(editor) => {
                self.remember_here();
                self.take_the_document(editor, std::path::PathBuf::new(), 0);
                self.editor.say(Message::StartedANewDocument);
            }
            Err(reason) => self.editor.say(Message::Plain(reason)),
        }
    }

    pub(crate) fn collect_open(&mut self, ctx: &egui::Context) {
        let Some(loading) = &self.loading else { return };
        if !loading.handle.is_finished() {
            ctx.request_repaint();
            return;
        }
        let Some(loading) = self.loading.take() else {
            return;
        };
        match loading.handle.join() {
            Ok(Opened::Document(editor)) => {
                self.take_the_document(*editor, loading.path, loading.page);
                if loading.changed_protection {
                    self.protection_changed = true;
                    self.editor.say(Message::Plain(
                        pdf_app::wording::Fact::ProtectionChanged.say(self.lang),
                    ));
                }
            }
            Ok(Opened::Locked(source)) => {
                self.editor.say(Message::Quiet);
                self.unlocking = Some(crate::unlock::Unlock {
                    path: loading.path,
                    page: loading.page,
                    source,
                    typed: String::new(),
                    tried: loading.tried_a_password,
                    shown: false,
                    focus: true,
                    for_pages: None,
                });
            }
            Ok(Opened::Refused(reason)) => self.editor.say(Message::Plain(reason)),
            Err(_) => {
                let failed = Message::OpeningFailed;
                self.editor.say(failed);
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn save(&mut self) -> bool {
        if self.input.pending() || self.input.draft().is_some() {
            self.editor.say(Message::ResolveDraftBeforeSaving);
            return false;
        }
        #[cfg(target_os = "android")]
        if self.untitled
            && self.destination.as_os_str().is_empty()
            && let Some(folder) = crate::android::cache_dir()
        {
            self.destination = folder.join("document.pdf");
        }
        if self.untitled && self.destination.as_os_str().is_empty() {
            self.save_a_copy_as();
            return false;
        }
        match self.editor.export() {
            Ok(export) => match crate::save_file::save(
                &self.opened,
                &self.destination,
                &export.bytes,
                self.saved_digest.as_deref(),
            ) {
                Ok(digest) => {
                    self.saved_digest = Some(digest);
                    self.saved_epoch = self.editor.epoch();
                    self.protection_changed = false;
                    if self.untitled {
                        self.title = self.destination.display().to_string();
                        let saved = self.destination.clone();
                        self.remember_file(&saved);
                    }
                    let frame = self.frame;
                    let revision = self
                        .editor
                        .revision()
                        .map_or_else(|| "unknown".to_owned(), |revision| format!("r{revision}"));
                    let digest = pdf_content::sha256_hex(&export.bytes);
                    if let Some(trace) = self.trace.as_mut() {
                        trace.note(
                            frame,
                            "saved",
                            &format!(
                                "path={} bytes={} revision={revision} sha256={digest}",
                                self.destination.display(),
                                export.bytes.len()
                            ),
                        );
                    }
                    #[cfg(not(target_arch = "wasm32"))]
                    crate::reporting::say(
                        pdf_app::trouble::Kind::Document,
                        &format!(
                            "saved {} ({} bytes)",
                            self.destination.display(),
                            export.bytes.len()
                        ),
                    );
                    let said = Message::SavedTo {
                        name: self.destination.display().to_string(),
                        bytes: export.bytes.len() as u64,
                    };
                    self.editor.say(said);
                    #[cfg(not(target_arch = "wasm32"))]
                    self.ai.saved_as(place_of(&self.destination));
                    #[cfg(target_os = "android")]
                    crate::android::export(&self.destination);
                    true
                }
                Err(error) => {
                    #[cfg(not(target_arch = "wasm32"))]
                    crate::reporting::say(
                        pdf_app::trouble::Kind::Failed,
                        &format!("could not save {}: {error}", self.destination.display()),
                    );
                    let said = Message::CouldNotSave {
                        name: self.destination.display().to_string(),
                        why: error.to_string(),
                    };
                    self.editor.say(said);
                    false
                }
            },
            Err(reason) => {
                self.editor.say(Message::Plain(reason));
                false
            }
        }
    }

    pub(crate) fn unsaved(&self) -> bool {
        self.editor.is_busy()
            || self.protection_changed
            || self.editor.epoch() != self.saved_epoch
            || self.input.pending()
            || self.input.draft().is_some()
            || self
                .live
                .as_ref()
                .is_some_and(crate::live_typing::LiveTyping::holds_typing)
    }

    pub(crate) fn guard_close(&mut self, ctx: &egui::Context) {
        if ctx.input(|input| input.viewport().close_requested())
            && !self.close_confirmed
            && (self.unsaved() || self.loading.is_some())
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.leaving = Some(Leaving::Close);
        }
    }

    pub(crate) fn asks_about_restrictions(&self) -> bool {
        !self.restriction_answered
            && !self.home
            && self.leaving.is_none()
            && self.loading.is_none()
            && self.unlocking.is_none()
            && self.editor.editing_restricted()
    }

    pub(crate) fn warn_of_restrictions(&mut self, ctx: &egui::Context) {
        if !self.asks_about_restrictions() {
            return;
        }
        let idle = !self.editor.is_busy();
        let mut edit = false;
        let mut read = false;
        let modal = egui::Modal::new(egui::Id::new("editing-restricted")).show(ctx, |ui| {
            ui.set_max_width(crate::side_panel::box_width(ui.ctx(), 460.0));
            ui.heading(Message::EditingRestricted.say(self.lang));
            ui.label(Message::EditingRestrictedWarning.say(self.lang));
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                edit = ui
                    .add_enabled(idle, egui::Button::new(Message::EditAnyway.say(self.lang)))
                    .clicked();
                read = ui.button(Message::ReadOnly.say(self.lang)).clicked();
            });
        });
        if (edit && self.editor.set_aside_restrictions()) || read || modal.should_close() {
            self.restriction_answered = true;
        }
    }

    pub(crate) fn confirm_leaving(&mut self, ctx: &egui::Context) {
        if self.leaving.is_none() {
            return;
        }
        let busy = self.editor.is_busy();
        let loading = self.loading.is_some();
        let pending = self.input.pending();
        let draft = self.input.draft().is_some();
        let offer = leaving_offer(StillHolding {
            at_rest: !busy && !loading && !pending,
            draft,
        });
        let mut choice = None;
        egui::Modal::new(egui::Id::new("unsaved-document")).show(ctx, |ui| {
            ui.heading(Message::UnsavedChanges.say(self.lang));
            ui.label(Message::SaveBeforeLeaving.say(self.lang));
            if draft {
                ui.label(Message::ResolveDraftBeforeSaving.say(self.lang));
            }
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        offer.save,
                        egui::Button::new(Message::Command(Command::Save).say(self.lang)),
                    )
                    .on_disabled_hover_text(
                        if busy {
                            Message::SaveWaitsForTheRunningEdit
                        } else if pending {
                            Message::SaveWaitsForTypingToLand
                        } else if draft {
                            Message::SaveWaitsForTheDraft
                        } else {
                            Message::SaveWaitsForTheDocumentToOpen
                        }
                        .say(self.lang),
                    )
                    .clicked()
                {
                    choice = Some(LeaveChoice::Save);
                }
                if ui
                    .add_enabled(
                        offer.discard,
                        egui::Button::new(Message::DiscardChanges.say(self.lang)),
                    )
                    .clicked()
                {
                    choice = Some(LeaveChoice::Discard);
                }
                if ui.button(Message::CancelLeaving.say(self.lang)).clicked() {
                    choice = Some(LeaveChoice::Cancel);
                }
            });
        });
        if let Some(choice) = choice {
            self.resolve_leaving(choice, ctx);
        }
    }

    pub(crate) fn resolve_leaving(&mut self, choice: LeaveChoice, ctx: &egui::Context) {
        match choice {
            LeaveChoice::Cancel => self.leaving = None,
            LeaveChoice::Save if !self.save() => {
                if self.chooser.is_some() {
                    self.leaving = None;
                }
            }
            LeaveChoice::Save | LeaveChoice::Discard => {
                if matches!(choice, LeaveChoice::Discard) {
                    self.input.abandon();
                }
                match self.leaving.take() {
                    Some(Leaving::Open(path, page)) => self.open_now(&path, page),
                    #[cfg(target_arch = "wasm32")]
                    Some(Leaving::OpenGiven(name, bytes)) => self.open_given_now(&name, bytes, ctx),
                    Some(Leaving::New(size)) => self.start_a_new_document(size),
                    Some(Leaving::Close) => {
                        self.close_confirmed = true;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    Some(Leaving::LetGo) => self.drop_the_document(),
                    None => {}
                }
            }
        }
    }

    pub(crate) fn status_bar(&self, ui: &mut egui::Ui) {
        let status = self.editor.status().say(self.lang);
        let hint = self.tool_hint();
        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                if let Some(hint) = hint {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(egui::RichText::new(hint).color(ui.visuals().weak_text_color()));
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            ui.add(egui::Label::new(&status).truncate())
                                .on_hover_text(&status);
                        });
                    });
                } else {
                    ui.add(egui::Label::new(&status).truncate())
                        .on_hover_text(&status);
                }
            });
        });
    }

    pub(crate) fn draft_bar(&mut self, ui: &mut egui::Ui) {
        let Some(draft) = self.input.draft() else {
            return;
        };
        let text = draft.text.clone();
        let reason = draft.reason.say(self.lang);
        let retryable = draft.can_retry(self.editor.epoch());
        let idle = !self.editor.is_busy() && self.resume.is_none();
        let preview: String = {
            let mut shown: String = text
                .chars()
                .take(40)
                .map(|character| match character {
                    '\n' => '⏎',
                    pdf_edit::LINE_BREAK => '↵',
                    other => other,
                })
                .collect();
            if text.chars().count() > 40 {
                shown.push('…');
            }
            shown
        };
        let lang = self.lang;
        let (mut retry, mut copy, mut discard) = (false, false, false);
        egui::Panel::top("draft").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(Message::DraftBarTitle.say(lang)).strong());
                ui.label(format!("“{preview}”")).on_hover_text(&text);
                retry = ui
                    .add_enabled(
                        retryable && idle,
                        egui::Button::new(Message::DraftRetry.say(lang)),
                    )
                    .on_disabled_hover_text(
                        if retryable {
                            Message::DraftWaitForTheEditToLand
                        } else {
                            Message::DraftPlaceNoLongerUsable
                        }
                        .say(lang),
                    )
                    .clicked();
                copy = ui.button(Message::DraftCopy.say(lang)).clicked();
                discard = ui.button(Message::DraftDiscard.say(lang)).clicked();
                ui.add(egui::Label::new(&reason).truncate())
                    .on_hover_text(&reason);
            });
        });
        if copy {
            ui.ctx().copy_text(text);
            self.editor.say(Message::DraftCopied);
        }
        if retry {
            self.retry_draft();
        }
        if discard {
            self.input.discard();
            let said = Message::DraftDiscarded;
            self.editor.say(said);
        }
    }
}

pub(crate) struct LeavingOffer {
    pub(crate) save: bool,
    pub(crate) discard: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct StillHolding {
    pub(crate) at_rest: bool,
    pub(crate) draft: bool,
}

pub(crate) fn leaving_offer(holding: StillHolding) -> LeavingOffer {
    LeavingOffer {
        save: holding.at_rest && !holding.draft,
        discard: true,
    }
}

fn toolbar_separator(ui: &mut egui::Ui) {
    ui.add_space(4.0);
    ui.separator();
    ui.add_space(4.0);
}

const STRIP_BELOW: f32 = 600.0;

const ALWAYS_RANK: u8 = 0;
const THEME_RANK: u8 = 7;
const HOME_RANK: u8 = 6;
const OPEN_RANK: u8 = 5;
const ZOOM_RANK: u8 = 5;
const SAVE_RANK: u8 = 4;
const PAGING_RANK: u8 = 4;
const DELETE_RANK: u8 = 3;
const RARE_TOOL_RANK: u8 = 2;
const HISTORY_RANK: u8 = 1;

const fn tool_rank(tool: Tool) -> u8 {
    match tool {
        Tool::Shape | Tool::Form | Tool::Link | Tool::Highlighter | Tool::Picture => RARE_TOOL_RANK,
        _ => 0,
    }
}

const TOOLS: [(Icon, Command, Tool); 8] = [
    (Icon::Select, Command::Select, Tool::Select),
    (Icon::Text, Command::Text, Tool::Text),
    (Icon::Pen, Command::Pen, Tool::Pen),
    (Icon::Highlighter, Command::Highlighter, Tool::Highlighter),
    (Icon::Shape, Command::Shape, Tool::Shape),
    (Icon::Form, Command::Form, Tool::Form),
    (Icon::Link, Command::Link, Tool::Link),
    (Icon::Picture, Command::Picture, Tool::Picture),
];

impl Slot {
    fn belongs_in_the_strip(&self) -> bool {
        let What::Button { command, .. } = self.what else {
            return false;
        };
        TOOLS.iter().any(|(_, tool, _)| *tool == command)
            || matches!(
                command,
                Command::StampWatermark | Command::Ocr | Command::RecognizeText | Command::Delete
            )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Side {
    Left,
    Right,
}

#[derive(Clone, Debug)]
enum What {
    Button {
        icon: Icon,
        command: Command,
        enabled: bool,
        on: bool,
    },
    Rule,
    Readout {
        text: String,
        width: f32,
    },
}

#[derive(Clone, Debug)]
struct Slot {
    side: Side,
    rank: u8,
    what: What,
}

impl Slot {
    fn is_button(&self) -> bool {
        matches!(self.what, What::Button { .. })
    }

    fn is_rule(&self) -> bool {
        matches!(self.what, What::Rule)
    }

    fn readout_width(&self) -> f32 {
        match self.what {
            What::Readout { width, .. } => width,
            _ => 0.0,
        }
    }

    fn piece(&self, labels: bool) -> room::Piece {
        let width = match self.what {
            What::Button { .. } => room::tool_width(labels) + room::GAP,
            What::Rule => room::RULE_WIDTH,
            What::Readout { width, .. } => width + room::GAP,
        };
        room::Piece {
            width,
            rank: self.rank,
        }
    }
}

fn text_width(ui: &egui::Ui, text: &str) -> f32 {
    let font = egui::TextStyle::Body.resolve(ui.style());
    ui.painter()
        .layout_no_wrap(text.to_owned(), font, egui::Color32::PLACEHOLDER)
        .size()
        .x
}

fn language_rows() -> Vec<(Lang, &'static str)> {
    Lang::ALL
        .iter()
        .map(|language| (*language, language.endonym()))
        .collect()
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn place_of(path: &std::path::Path) -> String {
    if path.as_os_str().is_empty() {
        return String::new();
    }
    std::fs::canonicalize(path)
        .or_else(|_| std::path::absolute(path))
        .unwrap_or_else(|_| path.to_path_buf())
        .display()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::{Lang, StillHolding, language_rows, leaving_offer};

    #[test]
    fn the_picker_lists_exactly_the_languages_the_window_speaks() {
        let rows = language_rows();
        assert_eq!(rows.len(), Lang::ALL.len());
        for (at, (language, named)) in rows.iter().enumerate() {
            assert_eq!(*language, Lang::ALL[at]);
            assert_eq!(*named, language.endonym());
            assert!(!named.is_empty());
            assert_eq!(Lang::of_tag(language.tag()), Some(*language));
        }
        let english = rows
            .iter()
            .find(|(language, _)| *language == Lang::English)
            .expect("the window speaks English");
        assert_eq!(english.1, "English");
    }

    fn old_buggy_offer(holding: StillHolding) -> (bool, bool) {
        (holding.at_rest && !holding.draft, holding.at_rest)
    }

    #[test]
    fn the_old_rule_was_a_dead_end_which_is_the_control() {
        let (save, discard) = old_buggy_offer(StillHolding::default());
        assert!(
            !save && !discard,
            "known answer: a busy editor alone used to disable both ways out"
        );
    }

    #[test]
    fn the_leaving_dialogue_always_offers_a_way_out() {
        let bools = [false, true];
        let mut raised_the_dialogue_at_least_once = false;
        for busy in bools {
            for pending in bools {
                for draft in bools {
                    for loading in bools {
                        for protection_changed in bools {
                            for epoch_moved in bools {
                                let unsaved =
                                    busy || protection_changed || epoch_moved || pending || draft;
                                if !(unsaved || loading) {
                                    continue;
                                }
                                raised_the_dialogue_at_least_once = true;
                                let offer = leaving_offer(StillHolding {
                                    at_rest: !busy && !loading && !pending,
                                    draft,
                                });
                                assert!(
                                    offer.discard,
                                    "Discard must never be disabled: busy={busy} \
                                     pending={pending} draft={draft} loading={loading}"
                                );
                                assert!(
                                    offer.save || offer.discard,
                                    "no way out at all: busy={busy} pending={pending} \
                                     draft={draft} loading={loading} \
                                     protection_changed={protection_changed} \
                                     epoch_moved={epoch_moved}"
                                );
                            }
                        }
                    }
                }
            }
        }
        assert!(raised_the_dialogue_at_least_once);
    }
}
