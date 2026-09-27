use eframe::egui;

use pdf_app::wording::Message;
use pdf_edit::form::{FieldKind, FormField};
use pdf_edit::tab_order::{TabOrder, in_reading_order};
use pdf_syntax::Reference;

use crate::window_state::{ChosenFields, Tool, Window};

const PANEL_WIDTH: f32 = 230.0;

const fn kind_name(kind: FieldKind) -> Message {
    match kind {
        FieldKind::Text => Message::FieldText,
        FieldKind::Checkbox => Message::FieldCheckbox,
        FieldKind::Radio => Message::FieldRadio,
        FieldKind::Combo => Message::FieldDropdown,
        FieldKind::List => Message::FieldListBox,
        FieldKind::Push => Message::FieldButton,
        FieldKind::Signature => Message::FieldSignature,
    }
}

#[derive(Default)]
struct Asked {
    go_to: Option<(usize, Reference)>,
    order: Option<(usize, bool)>,
    move_by: Option<(usize, Reference, isize)>,
}

impl Window {
    pub(crate) fn fields_panel(&mut self, ui: &mut egui::Ui) {
        if self.tool != Tool::Form {
            return;
        }
        let lang = self.lang;
        let fields = self.editor.fields_in_document();
        let phone = self.tools_in_a_strip(ui.ctx());
        if phone && fields.is_empty() {
            return;
        }
        let panel = if phone {
            egui::Panel::bottom("fields panel").resizable(false)
        } else {
            egui::Panel::right("fields panel")
                .resizable(false)
                .exact_size(PANEL_WIDTH)
        };
        let mut asked = Asked::default();
        panel.show(ui, |ui| {
            if phone {
                egui::CollapsingHeader::new(format!(
                    "{} \u{00b7} {}",
                    Message::FieldsPanel.say(lang),
                    fields.len()
                ))
                .default_open(false)
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(220.0)
                        .show(ui, |ui| self.field_rows(ui, &fields, &mut asked));
                });
                return;
            }
            ui.add_space(4.0);
            ui.heading(Message::FieldsPanel.say(lang));
            if fields.is_empty() {
                ui.label(Message::NoFieldsYet.say(lang));
                return;
            }
            egui::ScrollArea::vertical().show(ui, |ui| self.field_rows(ui, &fields, &mut asked));
        });
        let Asked {
            go_to,
            order,
            move_by,
        } = asked;
        if let Some((page, widget)) = go_to {
            self.goto(page);
            self.chosen_fields = Some(ChosenFields {
                page,
                widgets: vec![widget],
            });
        }
        if let Some((page, columns)) = order {
            self.order_the_fields(page, columns);
        }
        if let Some((page, widget, by)) = move_by {
            self.move_in_the_tab_order(page, widget, by);
        }
    }

    fn field_rows(&self, ui: &mut egui::Ui, fields: &[(usize, FormField)], asked: &mut Asked) {
        let lang = self.lang;
        let Asked {
            go_to,
            order,
            move_by,
        } = asked;
        let mut page = usize::MAX;
        for (at, (on, field)) in fields.iter().enumerate() {
            if *on != page {
                page = *on;
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(
                        Message::PageOf {
                            page: page + 1,
                            count: self.editor.page_count(),
                        }
                        .say(lang),
                    );
                    if ui
                        .small_button(Message::OrderByRow.say(lang))
                        .on_hover_text(Message::OrderByRowSaid.say(lang))
                        .clicked()
                    {
                        *order = Some((page, false));
                    }
                    if ui
                        .small_button(Message::OrderByColumn.say(lang))
                        .on_hover_text(Message::OrderByColumnSaid.say(lang))
                        .clicked()
                    {
                        *order = Some((page, true));
                    }
                });
                ui.separator();
            }
            let chosen = self
                .chosen_fields
                .as_ref()
                .is_some_and(|held| held.page == page && held.widgets.contains(&field.widget));
            ui.horizontal(|ui| {
                let label = format!(
                    "{}. {}  ·  {}",
                    at + 1,
                    field.name,
                    kind_name(field.kind).say(lang)
                );
                if ui.selectable_label(chosen, label).clicked() {
                    *go_to = Some((page, field.widget));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("▼").clicked() {
                        *move_by = Some((page, field.widget, 1_isize));
                    }
                    if ui.small_button("▲").clicked() {
                        *move_by = Some((page, field.widget, -1));
                    }
                });
            });
        }
    }

    fn tabbed(&mut self, page: usize) -> Vec<(Reference, [f64; 4])> {
        self.editor
            .fields_in_document()
            .iter()
            .filter(|(on, _)| *on == page)
            .map(|(_, field): &(usize, FormField)| (field.widget, field.rect))
            .collect()
    }

    fn order_the_fields(&mut self, page: usize, columns: bool) {
        let fields = self.tabbed(page);
        let widgets = in_reading_order(&fields, columns);
        self.write_the_order(page, widgets, TabOrder::AsListed);
    }

    fn move_in_the_tab_order(&mut self, page: usize, widget: Reference, by: isize) {
        let mut widgets: Vec<Reference> = self
            .tabbed(page)
            .into_iter()
            .map(|(widget, _)| widget)
            .collect();
        let Some(at) = widgets.iter().position(|held| *held == widget) else {
            return;
        };
        let to = at.saturating_add_signed(by);
        if to >= widgets.len() {
            return;
        }
        widgets.swap(at, to);
        self.write_the_order(page, widgets, TabOrder::AsListed);
    }

    fn write_the_order(&mut self, page: usize, widgets: Vec<Reference>, order: TabOrder) {
        if widgets.is_empty() {
            return;
        }
        let job = self.editor.begin_set_tab_order(page, widgets, order);
        if job.is_none() {
            self.editor.say(Message::AnotherEditIsRunning);
            return;
        }
        self.send(job);
    }
}
