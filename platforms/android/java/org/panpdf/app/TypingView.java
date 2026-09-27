package org.panpdf.app;

import android.content.Context;
import android.text.InputType;
import android.view.KeyEvent;
import android.view.View;
import android.view.inputmethod.BaseInputConnection;
import android.view.inputmethod.EditorInfo;
import android.view.inputmethod.InputConnection;

final class TypingView extends View {

    static native void nativeTyped(String text);

    static native void nativeKey(int code);

    TypingView(Context context) {
        super(context);
        setFocusable(true);
        setFocusableInTouchMode(true);
    }

    @Override
    public boolean onCheckIsTextEditor() {
        return true;
    }

    @Override
    public InputConnection onCreateInputConnection(EditorInfo out) {

        out.inputType = InputType.TYPE_CLASS_TEXT | InputType.TYPE_TEXT_FLAG_MULTI_LINE
                | InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS;
        out.imeOptions = EditorInfo.IME_FLAG_NO_EXTRACT_UI | EditorInfo.IME_FLAG_NO_FULLSCREEN;
        return new Typing(this);
    }

    private static final class Typing extends BaseInputConnection {

        private String composing = "";

        Typing(View view) {
            super(view, false);
        }

        @Override
        public boolean commitText(CharSequence text, int newCursorPosition) {
            become(text.toString());
            composing = "";
            return true;
        }

        @Override
        public boolean setComposingText(CharSequence text, int newCursorPosition) {
            become(text.toString());
            composing = text.toString();
            return true;
        }

        @Override
        public boolean finishComposingText() {
            composing = "";
            return true;
        }

        @Override
        public boolean deleteSurroundingText(int before, int after) {
            for (int i = 0; i < before; i++) {
                nativeKey(KeyEvent.KEYCODE_DEL);
            }
            for (int i = 0; i < after; i++) {
                nativeKey(KeyEvent.KEYCODE_FORWARD_DEL);
            }
            return true;
        }

        @Override
        public boolean deleteSurroundingTextInCodePoints(int before, int after) {
            return deleteSurroundingText(before, after);
        }

        @Override
        public boolean sendKeyEvent(KeyEvent event) {
            if (event.getAction() != KeyEvent.ACTION_DOWN) {
                return true;
            }
            switch (event.getKeyCode()) {
                case KeyEvent.KEYCODE_DEL:
                case KeyEvent.KEYCODE_FORWARD_DEL:
                case KeyEvent.KEYCODE_ENTER:
                case KeyEvent.KEYCODE_DPAD_LEFT:
                case KeyEvent.KEYCODE_DPAD_RIGHT:
                case KeyEvent.KEYCODE_DPAD_UP:
                case KeyEvent.KEYCODE_DPAD_DOWN:
                case KeyEvent.KEYCODE_TAB:
                    nativeKey(event.getKeyCode());
                    return true;
                default:
                    int unicode = event.getUnicodeChar();
                    if (unicode != 0) {
                        nativeTyped(new String(Character.toChars(unicode)));
                    }
                    return true;
            }
        }

        @Override
        public CharSequence getTextBeforeCursor(int length, int flags) {
            return "";
        }

        @Override
        public CharSequence getTextAfterCursor(int length, int flags) {
            return "";
        }

        private void become(String next) {
            int common = 0;
            int limit = Math.min(composing.length(), next.length());
            while (common < limit && composing.charAt(common) == next.charAt(common)) {
                common++;
            }
            int taken = composing.codePointCount(common, composing.length());
            for (int i = 0; i < taken; i++) {
                nativeKey(KeyEvent.KEYCODE_DEL);
            }
            if (common < next.length()) {
                nativeTyped(next.substring(common));
            }
        }
    }
}
