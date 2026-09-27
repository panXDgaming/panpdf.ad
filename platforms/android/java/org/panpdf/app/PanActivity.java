package org.panpdf.app;

import android.app.NativeActivity;
import android.content.ClipData;
import android.content.ClipboardManager;
import android.graphics.Rect;
import android.view.ActionMode;
import android.view.Menu;
import android.view.MenuItem;
import android.content.Intent;
import android.database.Cursor;
import android.graphics.Insets;
import android.net.Uri;
import android.os.Build;
import android.os.Bundle;
import android.provider.OpenableColumns;
import android.view.View;
import android.view.WindowInsets;

import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.InputStream;
import java.io.OutputStream;

public class PanActivity extends NativeActivity {
    static {
        System.loadLibrary("panpdf");
    }

    private static final int PICK_PDF = 1;
    private static final int EXPORT = 2;
    private static final int SCAN = 3;
    private static final int TOOL_FILES = 4;

    private String choosingFor;

    private static native void nativeToolFiles(String tool, String[] paths);

    private boolean scanningInto;

    private static native void nativeScanned(String[] paths, boolean into);

    private TypingView typing;

    private static native void nativePaths(String libraries, String files);

    private String exporting;

    private static native void nativeInsets(int top, int bottom, int left, int right);

    private static native void nativeOpened(String path);

    private static native void nativeSaid(String what);

    private static native void nativeCacheDir(String path);

    private static native void nativeCommand(String what);

    private static native void nativePaste(String text);

    private static final int COPY = 1;
    private static final int CUT = 2;
    private static final int PASTE = 3;
    private static final int SELECT_ALL = 4;

    private ActionMode menu;
    private boolean menuEditing;
    private final Rect menuRect = new Rect();

    @Override
    protected void onCreate(Bundle state) {

        unpackFonts();
        nativeCacheDir(getCacheDir().getAbsolutePath());
        nativePaths(getApplicationInfo().nativeLibraryDir, getFilesDir().getAbsolutePath());
        super.onCreate(state);

        if (Build.VERSION.SDK_INT >= 30 && getWindow().getInsetsController() != null) {
            int light = android.view.WindowInsetsController.APPEARANCE_LIGHT_STATUS_BARS
                    | android.view.WindowInsetsController.APPEARANCE_LIGHT_NAVIGATION_BARS;
            getWindow().getInsetsController().setSystemBarsAppearance(light, light);
        }
        typing = new TypingView(this);
        addContentView(typing, new android.view.ViewGroup.LayoutParams(1, 1));
        View decor = getWindow().getDecorView();
        decor.setOnApplyWindowInsetsListener((view, insets) -> {
            int top;
            int bottom;
            int left;
            int right;
            if (Build.VERSION.SDK_INT >= 30) {
                Insets bars = insets.getInsets(WindowInsets.Type.systemBars()
                        | WindowInsets.Type.displayCutout() | WindowInsets.Type.ime());
                top = bars.top;
                bottom = bars.bottom;
                left = bars.left;
                right = bars.right;
            } else {
                top = insets.getSystemWindowInsetTop();
                bottom = insets.getSystemWindowInsetBottom();
                left = insets.getSystemWindowInsetLeft();
                right = insets.getSystemWindowInsetRight();
            }
            nativeInsets(top, bottom, left, right);
            return view.onApplyWindowInsets(insets);
        });
        take(getIntent());
    }

    @Override
    protected void onNewIntent(Intent intent) {
        super.onNewIntent(intent);
        setIntent(intent);
        take(intent);
    }

    @Override
    @SuppressWarnings("deprecation")
    public void onBackPressed() {
        nativeCommand("back");
    }

    public void openUrl(String address) {
        runOnUiThread(() -> {
            try {
                startActivity(new Intent(Intent.ACTION_VIEW, Uri.parse(address)));
            } catch (Exception failure) {
                nativeSaid("not opened: " + failure.getMessage());
            }
        });
    }

    public void showKeyboard(boolean up) {
        runOnUiThread(() -> {
            android.view.inputmethod.InputMethodManager keyboard =
                    getSystemService(android.view.inputmethod.InputMethodManager.class);
            if (keyboard == null) {
                return;
            }
            if (up) {
                typing.requestFocus();
                keyboard.restartInput(typing);
                keyboard.showSoftInput(typing, 0);
            } else {
                keyboard.hideSoftInputFromWindow(typing.getWindowToken(), 0);
                typing.clearFocus();
            }
        });
    }

    public void leave() {
        runOnUiThread(() -> moveTaskToBack(true));
    }

    public void setClipboard(String text) {
        runOnUiThread(() -> {
            ClipboardManager clipboard = getSystemService(ClipboardManager.class);
            if (clipboard != null) {
                clipboard.setPrimaryClip(ClipData.newPlainText("PanPDF", text));
            }
        });
    }

    public void showMenu(int left, int top, int right, int bottom, boolean editing) {
        runOnUiThread(() -> {
            menuRect.set(left, top, right, bottom);
            if (menu != null && menuEditing == editing) {
                menu.invalidateContentRect();
                return;
            }
            if (menu != null) {
                menu.finish();
            }
            menuEditing = editing;
            menu = getWindow().getDecorView().startActionMode(new ActionMode.Callback2() {
                @Override
                public boolean onCreateActionMode(ActionMode mode, Menu items) {
                    if (editing) {
                        items.add(Menu.NONE, CUT, 0, android.R.string.cut);
                    }
                    items.add(Menu.NONE, COPY, 1, android.R.string.copy);
                    if (editing) {
                        items.add(Menu.NONE, PASTE, 2, android.R.string.paste);
                    }
                    items.add(Menu.NONE, SELECT_ALL, 3, android.R.string.selectAll);
                    return true;
                }

                @Override
                public boolean onPrepareActionMode(ActionMode mode, Menu items) {
                    return false;
                }

                @Override
                public boolean onActionItemClicked(ActionMode mode, MenuItem item) {
                    switch (item.getItemId()) {
                        case COPY:
                            nativeCommand("copy");
                            break;
                        case CUT:
                            nativeCommand("cut");
                            break;
                        case PASTE:
                            ClipboardManager clipboard = getSystemService(ClipboardManager.class);
                            if (clipboard != null && clipboard.hasPrimaryClip()
                                    && clipboard.getPrimaryClip().getItemCount() > 0) {
                                CharSequence text = clipboard.getPrimaryClip().getItemAt(0)
                                        .coerceToText(PanActivity.this);
                                nativePaste(text.toString());
                            }
                            break;
                        case SELECT_ALL:
                            nativeCommand("selectAll");
                            return true;
                        default:
                            return false;
                    }
                    mode.finish();
                    return true;
                }

                @Override
                public void onDestroyActionMode(ActionMode mode) {
                    if (menu == mode) {

                        menu = null;
                        nativeCommand("menuGone");
                    }
                }

                @Override
                public void onGetContentRect(ActionMode mode, View view, Rect out) {
                    out.set(menuRect);
                }
            }, ActionMode.TYPE_FLOATING);
        });
    }

    public void hideMenu() {
        runOnUiThread(() -> {
            if (menu != null) {
                ActionMode closing = menu;
                menu = null;
                closing.finish();
            }
        });
    }

    private void unpackFonts() {
        File root = new File(getFilesDir(), "fonts");
        File stamp = new File(root, "version");
        String version;
        try {
            version = String.valueOf(getPackageManager().getPackageInfo(getPackageName(), 0).lastUpdateTime);
        } catch (Exception failure) {
            version = "unknown";
        }
        try {
            if (stamp.isFile() && version.equals(new String(java.nio.file.Files.readAllBytes(stamp.toPath())))) {
                return;
            }
            File packaged = new File(root, "packaged");
            packaged.mkdirs();
            android.content.res.AssetManager assets = getAssets();
            try (InputStream in = assets.open("fonts/manifest.json");
                 OutputStream out = new FileOutputStream(new File(root, "manifest.json"))) {
                copy(in, out);
            }
            String[] faces = assets.list("fonts/packaged");
            if (faces != null) {
                for (String face : faces) {
                    try (InputStream in = assets.open("fonts/packaged/" + face);
                         OutputStream out = new FileOutputStream(new File(packaged, face))) {
                        copy(in, out);
                    }
                }
            }
            try (OutputStream out = new FileOutputStream(stamp)) {
                out.write(version.getBytes());
            }
        } catch (Exception failure) {

        }
    }

    private void take(Intent intent) {
        if (intent == null || !Intent.ACTION_VIEW.equals(intent.getAction()) || intent.getData() == null) {
            return;
        }
        open(intent.getData());
    }

    public void pickPdf() {
        runOnUiThread(() -> {
            Intent intent = new Intent(Intent.ACTION_OPEN_DOCUMENT);
            intent.addCategory(Intent.CATEGORY_OPENABLE);
            intent.setType("application/pdf");
            startActivityForResult(intent, PICK_PDF);
        });
    }

    public static String download(String address, String path) {
        try {
            java.net.HttpURLConnection connection =
                    (java.net.HttpURLConnection) new java.net.URL(address).openConnection();
            connection.setConnectTimeout(20000);
            connection.setReadTimeout(60000);
            connection.setInstanceFollowRedirects(true);
            int status = connection.getResponseCode();
            if (status / 100 != 2) {
                return "the server answered " + status;
            }
            try (InputStream in = connection.getInputStream();
                 OutputStream out = new FileOutputStream(path)) {
                copy(in, out);
            }
            return null;
        } catch (Exception failure) {
            return String.valueOf(failure.getMessage());
        }
    }

    public void shareFile(String path) {
        runOnUiThread(() -> {
            File file = new File(path);
            Uri address = Uri.parse("content://" + FilesProvider.AUTHORITY + "/" + Uri.encode(file.getName()));
            Intent send = new Intent(Intent.ACTION_SEND);
            send.setType(typeOf(file.getName()));
            send.putExtra(Intent.EXTRA_STREAM, address);
            send.setClipData(android.content.ClipData.newRawUri(file.getName(), address));
            send.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION);
            try {
                startActivity(Intent.createChooser(send, null));
            } catch (Exception failure) {
                nativeSaid("not shared: " + failure.getMessage());
            }
        });
    }

    public void pickFiles(String types, boolean several, String tool) {
        runOnUiThread(() -> {
            choosingFor = tool;
            Intent intent = new Intent(Intent.ACTION_OPEN_DOCUMENT);
            intent.addCategory(Intent.CATEGORY_OPENABLE);
            String[] each = types.split(",");
            intent.setType(each.length == 1 ? each[0] : "*/*");
            intent.putExtra(Intent.EXTRA_MIME_TYPES, each);
            intent.putExtra(Intent.EXTRA_ALLOW_MULTIPLE, several);
            startActivityForResult(intent, TOOL_FILES);
        });
    }

    public void startScan(boolean into) {
        runOnUiThread(() -> {
            scanningInto = into;
            startActivityForResult(new Intent(this, ScanActivity.class), SCAN);
        });
    }

    public void exportFile(String path, String name) {
        runOnUiThread(() -> {
            exporting = path;
            Intent intent = new Intent(Intent.ACTION_CREATE_DOCUMENT);
            intent.addCategory(Intent.CATEGORY_OPENABLE);
            intent.setType(typeOf(name));
            intent.putExtra(Intent.EXTRA_TITLE, name);
            startActivityForResult(intent, EXPORT);
        });
    }

    @Override
    protected void onActivityResult(int request, int result, Intent data) {
        super.onActivityResult(request, result, data);
        if (request == SCAN) {
            if (result == RESULT_OK && data != null) {
                String[] pages = data.getStringArrayExtra(ScanActivity.PAGES);
                if (pages != null && pages.length > 0) {
                    nativeScanned(pages, scanningInto);
                }
            }
            return;
        }
        if (request == TOOL_FILES) {
            if (result == RESULT_OK && data != null && choosingFor != null) {
                java.util.List<Uri> chosen = new java.util.ArrayList<>();
                if (data.getClipData() != null) {
                    for (int i = 0; i < data.getClipData().getItemCount(); i++) {
                        chosen.add(data.getClipData().getItemAt(i).getUri());
                    }
                } else if (data.getData() != null) {
                    chosen.add(data.getData());
                }
                String tool = choosingFor;
                new Thread(() -> {
                    File folder = new File(getCacheDir(), "tool-in");
                    File[] old = folder.listFiles();
                    if (old != null) {
                        for (File file : old) {
                            file.delete();
                        }
                    }
                    folder.mkdirs();
                    java.util.List<String> paths = new java.util.ArrayList<>();
                    for (Uri uri : chosen) {
                        File file = new File(folder, anyNameOf(uri));
                        try (InputStream in = getContentResolver().openInputStream(uri);
                             OutputStream out = new FileOutputStream(file)) {
                            copy(in, out);
                            paths.add(file.getAbsolutePath());
                        } catch (Exception failure) {
                            nativeSaid("not opened: " + failure.getMessage());
                        }
                    }
                    nativeToolFiles(tool, paths.toArray(new String[0]));
                }).start();
            }
            return;
        }
        if (result != RESULT_OK || data == null || data.getData() == null) {
            return;
        }
        if (request == PICK_PDF) {
            open(data.getData());
        } else if (request == EXPORT && exporting != null) {
            String from = exporting;
            exporting = null;
            try (InputStream in = new FileInputStream(from);
                 OutputStream out = getContentResolver().openOutputStream(data.getData(), "wt")) {
                copy(in, out);
                nativeSaid("saved");
            } catch (Exception failure) {
                nativeSaid("not saved: " + failure.getMessage());
            }
        }
    }

    private void open(Uri uri) {
        new Thread(() -> {
            try {
                File folder = new File(getCacheDir(), "opened");
                folder.mkdirs();
                File file = new File(folder, nameOf(uri));
                try (InputStream in = getContentResolver().openInputStream(uri);
                     OutputStream out = new FileOutputStream(file)) {
                    copy(in, out);
                }
                nativeOpened(file.getAbsolutePath());
            } catch (Exception failure) {
                nativeSaid("not opened: " + failure.getMessage());
            }
        }).start();
    }

    private String anyNameOf(Uri uri) {
        String name = null;
        try (Cursor cursor = getContentResolver().query(uri, new String[] {OpenableColumns.DISPLAY_NAME},
                null, null, null)) {
            if (cursor != null && cursor.moveToFirst()) {
                name = cursor.getString(0);
            }
        } catch (Exception ignored) {

        }
        if (name == null || name.isEmpty()) {
            name = "file-" + System.nanoTime();
        }
        return name.replace('/', '_').replace('\\', '_');
    }

    static String typeOf(String name) {
        int dot = name.lastIndexOf('.');
        String found = dot < 0 ? null
                : android.webkit.MimeTypeMap.getSingleton().getMimeTypeFromExtension(name.substring(dot + 1).toLowerCase());
        return found == null ? "application/octet-stream" : found;
    }

    private String nameOf(Uri uri) {
        String name = null;
        try (Cursor cursor = getContentResolver().query(uri, new String[] {OpenableColumns.DISPLAY_NAME},
                null, null, null)) {
            if (cursor != null && cursor.moveToFirst()) {
                name = cursor.getString(0);
            }
        } catch (Exception ignored) {

        }
        if (name == null || name.isEmpty()) {
            name = uri.getLastPathSegment();
        }
        if (name == null || name.isEmpty()) {
            name = "document.pdf";
        }
        name = name.replace('/', '_').replace('\\', '_');
        return name.toLowerCase().endsWith(".pdf") ? name : name + ".pdf";
    }

    private static void copy(InputStream in, OutputStream out) throws java.io.IOException {
        byte[] buffer = new byte[1 << 16];
        int read;
        while ((read = in.read(buffer)) > 0) {
            out.write(buffer, 0, read);
        }
    }
}
