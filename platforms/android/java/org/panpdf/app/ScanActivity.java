package org.panpdf.app;

import android.Manifest;
import android.app.Activity;
import android.content.Context;
import android.content.Intent;
import android.content.pm.PackageManager;
import android.graphics.Bitmap;
import android.graphics.BitmapFactory;
import android.graphics.Canvas;
import android.graphics.Color;
import android.graphics.Matrix;
import android.graphics.Paint;
import android.graphics.Path;
import android.graphics.PointF;
import android.graphics.SurfaceTexture;
import android.graphics.drawable.GradientDrawable;
import android.hardware.Sensor;
import android.hardware.SensorEvent;
import android.hardware.SensorEventListener;
import android.hardware.SensorManager;
import android.hardware.camera2.CameraCaptureSession;
import android.hardware.camera2.CameraCharacteristics;
import android.hardware.camera2.CameraDevice;
import android.hardware.camera2.CameraManager;
import android.hardware.camera2.CaptureRequest;
import android.hardware.camera2.CaptureResult;
import android.hardware.camera2.TotalCaptureResult;
import android.hardware.camera2.params.StreamConfigurationMap;
import android.graphics.ImageFormat;
import android.media.Image;
import android.media.ImageReader;
import android.os.Bundle;
import android.os.Handler;
import android.os.HandlerThread;
import android.util.Log;
import android.util.Size;
import android.view.Gravity;
import android.view.MotionEvent;
import android.view.Surface;
import android.view.TextureView;
import android.view.View;
import android.widget.Button;
import android.widget.FrameLayout;
import android.widget.LinearLayout;
import android.widget.ProgressBar;
import android.widget.TextView;

import java.io.File;
import java.io.FileOutputStream;
import java.nio.ByteBuffer;
import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;

public class ScanActivity extends Activity {
    static {
        System.loadLibrary("panpdf");
    }

    public static final String PAGES = "pages";

    private static native int[] flattenSheet(int[] pixels, int width, int height, float[] corners, boolean clean);

    private static native float[] lookForSheet(int[] pixels, int width, int height, int turns);

    private static native float[] cornersInPhoto(int[] pixels, int width, int height);

    private static final int ASK_CAMERA = 7;
    private static final int BLUE = Color.rgb(0, 90, 200);
    private static final int GREEN = Color.rgb(20, 150, 70);

    private static final long STEADY_MS = 900;

    private static final float STILL = 0.015f;

    private static final float SHAKING = 0.25f;

    private static final long SETTLED_MS = 300;

    private static final long FOCUS_MS = 1500;

    private static final float SURE = 0.5f;

    private static final long RECENT_MS = 1500;

    private static final long GONE_MS = 800;

    private static final float MOVED_ON = 0.15f;

    private static final int LOOKED_AT = 2048;

    private static final int PREVIEW = 0;
    private static final int FOCUSING = 1;
    private static final int TAKING = 2;

    private TextureView preview;
    private SheetView sheet;
    private TextView hint;
    private Button done;
    private Button autoButton;
    private Button lightButton;
    private FrameLayout root;

    private HandlerThread cameraThread;
    private Handler camera;
    private HandlerThread lookThread;
    private Handler look;
    private CameraDevice device;
    private CameraCaptureSession session;
    private CaptureRequest.Builder previewRequest;
    private ImageReader frames;
    private ImageReader photos;
    private int sensorTurn;
    private Size frameSize;
    private boolean canFocus;
    private boolean hasLight;

    private volatile int state = PREVIEW;
    private long focusStarted;
    private volatile boolean looking;
    private long lastLook;

    private int nextTurn;
    private final float[][] byTurn = new float[4][];
    private final float[] sureness = new float[4];
    private final long[] lookedAt = new long[4];
    private boolean auto = true;
    private boolean light;

    private boolean armed = true;

    private float[] takenAt;

    private long lastSeen;
    private boolean onCameraScreen = true;

    private View finding;

    private final ArrayDeque<float[]> recent = new ArrayDeque<>();
    private float[] lastShown;
    private long steadySince;
    private int misses;

    private SensorManager sensors;
    private volatile long lastShake;
    private final SensorEventListener gyroscope = new SensorEventListener() {
        @Override
        public void onSensorChanged(SensorEvent event) {
            float x = event.values[0];
            float y = event.values[1];
            float z = event.values[2];
            if (Math.sqrt(x * x + y * y + z * z) > SHAKING) {
                lastShake = System.currentTimeMillis();
            }
        }

        @Override
        public void onAccuracyChanged(Sensor sensor, int accuracy) {
        }
    };

    private final List<String> pages = new ArrayList<>();

    @Override
    protected void onCreate(Bundle state) {
        super.onCreate(state);
        root = new FrameLayout(this);
        root.setBackgroundColor(Color.BLACK);
        preview = new TextureView(this);
        root.addView(preview, new FrameLayout.LayoutParams(-1, -1));
        sheet = new SheetView(this);
        root.addView(sheet, new FrameLayout.LayoutParams(-1, -1));

        View shutter = new View(this);
        GradientDrawable ring = new GradientDrawable();
        ring.setShape(GradientDrawable.OVAL);
        ring.setColor(Color.WHITE);
        ring.setStroke(dp(4), Color.argb(140, 255, 255, 255));
        shutter.setBackground(ring);
        shutter.setOnClickListener(v -> takePicture());
        FrameLayout.LayoutParams shutterAt = new FrameLayout.LayoutParams(dp(76), dp(76),
                Gravity.BOTTOM | Gravity.CENTER_HORIZONTAL);
        shutterAt.bottomMargin = dp(36);
        root.addView(shutter, shutterAt);

        TextView close = new TextView(this);
        close.setText("✕");
        close.setTextColor(Color.WHITE);
        close.setTextSize(24);
        close.setGravity(Gravity.CENTER);
        close.setShadowLayer(dp(3), 0, 0, Color.argb(160, 0, 0, 0));
        close.setOnClickListener(v -> finishWithPages(false));
        FrameLayout.LayoutParams closeAt = new FrameLayout.LayoutParams(dp(56), dp(56), Gravity.TOP | Gravity.START);
        closeAt.topMargin = dp(12);
        closeAt.leftMargin = dp(8);
        root.addView(close, closeAt);

        LinearLayout tools = new LinearLayout(this);
        tools.setGravity(Gravity.CENTER_VERTICAL);
        autoButton = pill("Auto");
        autoButton.setOnClickListener(v -> {
            auto = !auto;
            showToggles();
        });
        lightButton = pill("Light");
        lightButton.setOnClickListener(v -> {
            light = !light;
            showToggles();
            repeatPreview();
        });
        tools.addView(autoButton, new LinearLayout.LayoutParams(-2, dp(40)));
        LinearLayout.LayoutParams lightAt = new LinearLayout.LayoutParams(-2, dp(40));
        lightAt.leftMargin = dp(8);
        tools.addView(lightButton, lightAt);
        FrameLayout.LayoutParams toolsAt = new FrameLayout.LayoutParams(-2, -2, Gravity.TOP | Gravity.CENTER_HORIZONTAL);
        toolsAt.topMargin = dp(20);
        root.addView(tools, toolsAt);

        hint = new TextView(this);
        hint.setTextColor(Color.WHITE);
        hint.setTextSize(16);
        hint.setGravity(Gravity.CENTER);
        hint.setShadowLayer(dp(3), 0, 0, Color.argb(200, 0, 0, 0));
        FrameLayout.LayoutParams hintAt = new FrameLayout.LayoutParams(-2, -2, Gravity.BOTTOM | Gravity.CENTER_HORIZONTAL);
        hintAt.bottomMargin = dp(124);
        root.addView(hint, hintAt);

        done = new Button(this);
        done.setAllCaps(false);
        done.setTextColor(Color.WHITE);
        done.setTextSize(15);
        GradientDrawable blue = new GradientDrawable();
        blue.setColor(BLUE);
        blue.setCornerRadius(dp(22));
        done.setBackground(blue);
        done.setPadding(dp(18), 0, dp(18), 0);
        done.setOnClickListener(v -> finishWithPages(true));
        FrameLayout.LayoutParams doneAt = new FrameLayout.LayoutParams(-2, dp(44), Gravity.TOP | Gravity.END);
        doneAt.topMargin = dp(18);
        doneAt.rightMargin = dp(14);
        root.addView(done, doneAt);

        root.setOnApplyWindowInsetsListener((view, insets) -> {
            int top = insets.getSystemWindowInsetTop();
            int bottom = insets.getSystemWindowInsetBottom();
            closeAt.topMargin = dp(12) + top;
            doneAt.topMargin = dp(18) + top;
            toolsAt.topMargin = dp(20) + top;
            shutterAt.bottomMargin = dp(36) + bottom;
            hintAt.bottomMargin = dp(124) + bottom;
            close.setLayoutParams(closeAt);
            done.setLayoutParams(doneAt);
            tools.setLayoutParams(toolsAt);
            shutter.setLayoutParams(shutterAt);
            hint.setLayoutParams(hintAt);
            return insets;
        });
        setContentView(root);
        showCount();
        showToggles();
        sensors = getSystemService(SensorManager.class);

        if (checkSelfPermission(Manifest.permission.CAMERA) != PackageManager.PERMISSION_GRANTED) {
            requestPermissions(new String[] {Manifest.permission.CAMERA}, ASK_CAMERA);
        }
    }

    @Override
    public void onRequestPermissionsResult(int request, String[] permissions, int[] results) {
        if (request == ASK_CAMERA) {
            if (results.length > 0 && results[0] == PackageManager.PERMISSION_GRANTED) {
                startWhenReady();
            } else {
                finishWithPages(false);
            }
        }
    }

    @Override
    protected void onResume() {
        super.onResume();
        Sensor gyro = sensors == null ? null : sensors.getDefaultSensor(Sensor.TYPE_GYROSCOPE);
        if (gyro != null) {
            sensors.registerListener(gyroscope, gyro, SensorManager.SENSOR_DELAY_UI);
        }
        if (checkSelfPermission(Manifest.permission.CAMERA) == PackageManager.PERMISSION_GRANTED) {
            startWhenReady();
        }
    }

    @Override
    protected void onPause() {
        if (sensors != null) {
            sensors.unregisterListener(gyroscope);
        }
        stopCamera();
        super.onPause();
    }

    @Override
    @SuppressWarnings("deprecation")
    public void onBackPressed() {
        finishWithPages(!pages.isEmpty());
    }

    private void startWhenReady() {
        if (device != null) {
            return;
        }
        if (preview.isAvailable()) {
            startCamera();
        } else {
            preview.setSurfaceTextureListener(new TextureView.SurfaceTextureListener() {
                @Override
                public void onSurfaceTextureAvailable(SurfaceTexture texture, int w, int h) {
                    startCamera();
                }

                @Override
                public void onSurfaceTextureSizeChanged(SurfaceTexture texture, int w, int h) {
                    fitPreview();
                }

                @Override
                public boolean onSurfaceTextureDestroyed(SurfaceTexture texture) {
                    return true;
                }

                @Override
                public void onSurfaceTextureUpdated(SurfaceTexture texture) {
                }
            });
        }
    }

    private void startCamera() {
        try {
            cameraThread = new HandlerThread("scan camera");
            cameraThread.start();
            camera = new Handler(cameraThread.getLooper());

            lookThread = new HandlerThread("scan look");
            lookThread.start();
            look = new Handler(lookThread.getLooper());
            CameraManager manager = getSystemService(CameraManager.class);
            String chosen = null;
            for (String id : manager.getCameraIdList()) {
                Integer facing = manager.getCameraCharacteristics(id).get(CameraCharacteristics.LENS_FACING);
                if (facing != null && facing == CameraCharacteristics.LENS_FACING_BACK) {
                    chosen = id;
                    break;
                }
            }
            if (chosen == null) {
                String[] all = manager.getCameraIdList();
                if (all.length == 0) {
                    finishWithPages(false);
                    return;
                }
                chosen = all[0];
            }
            CameraCharacteristics about = manager.getCameraCharacteristics(chosen);
            Integer turn = about.get(CameraCharacteristics.SENSOR_ORIENTATION);
            sensorTurn = turn == null ? 90 : turn;
            Float nearest = about.get(CameraCharacteristics.LENS_INFO_MINIMUM_FOCUS_DISTANCE);
            canFocus = nearest != null && nearest > 0;
            Boolean flash = about.get(CameraCharacteristics.FLASH_INFO_AVAILABLE);
            hasLight = flash != null && flash;
            runOnUiThread(this::showToggles);
            StreamConfigurationMap map = about.get(CameraCharacteristics.SCALER_STREAM_CONFIGURATION_MAP);
            Size photo = largest(map.getOutputSizes(ImageFormat.JPEG));
            frameSize = near(map.getOutputSizes(ImageFormat.YUV_420_888), 640, 480, photo);
            photos = ImageReader.newInstance(photo.getWidth(), photo.getHeight(), ImageFormat.JPEG, 2);
            photos.setOnImageAvailableListener(this::photoTaken, camera);
            frames = ImageReader.newInstance(frameSize.getWidth(), frameSize.getHeight(), ImageFormat.YUV_420_888, 2);
            frames.setOnImageAvailableListener(this::lookForTheSheet, camera);
            fitPreview();
            manager.openCamera(chosen, new CameraDevice.StateCallback() {
                @Override
                public void onOpened(CameraDevice opened) {
                    device = opened;
                    startPreview();
                }

                @Override
                public void onDisconnected(CameraDevice gone) {
                    gone.close();
                    device = null;
                }

                @Override
                public void onError(CameraDevice failed, int error) {
                    Log.w("PanPDF", "scan: camera error " + error);
                    failed.close();
                    device = null;
                }
            }, camera);
        } catch (SecurityException | android.hardware.camera2.CameraAccessException failure) {
            Log.w("PanPDF", "scan: the camera did not open", failure);
            finishWithPages(false);
        }
    }

    private void startPreview() {
        startPreview(true);
    }

    @SuppressWarnings("deprecation")
    private void startPreview(boolean looking) {
        try {
            SurfaceTexture texture = preview.getSurfaceTexture();
            texture.setDefaultBufferSize(frameSize.getWidth(), frameSize.getHeight());
            Surface shown = new Surface(texture);
            CaptureRequest.Builder request = device.createCaptureRequest(CameraDevice.TEMPLATE_PREVIEW);
            request.addTarget(shown);
            if (looking) {
                request.addTarget(frames.getSurface());
            }
            request.set(CaptureRequest.CONTROL_AF_MODE, CaptureRequest.CONTROL_AF_MODE_CONTINUOUS_PICTURE);
            List<Surface> outputs = looking
                    ? Arrays.asList(shown, frames.getSurface(), photos.getSurface())
                    : Arrays.asList(shown, photos.getSurface());
            device.createCaptureSession(outputs,
                    new CameraCaptureSession.StateCallback() {
                        @Override
                        public void onConfigured(CameraCaptureSession configured) {
                            session = configured;
                            previewRequest = request;
                            repeatPreview();
                        }

                        @Override
                        public void onConfigureFailed(CameraCaptureSession failed) {
                            Log.w("PanPDF", "scan: the camera refused " + (looking ? "three" : "two") + " streams");
                            if (looking && device != null) {
                                startPreview(false);
                            } else {
                                finishWithPages(false);
                            }
                        }
                    }, camera);
        } catch (Exception failure) {
            Log.w("PanPDF", "scan: preview failed", failure);
            finishWithPages(false);
        }
    }

    private void repeatPreview() {
        CameraCaptureSession running = session;
        CaptureRequest.Builder request = previewRequest;
        if (running == null || request == null) {
            return;
        }
        try {
            request.set(CaptureRequest.CONTROL_AE_MODE, CaptureRequest.CONTROL_AE_MODE_ON);
            request.set(CaptureRequest.FLASH_MODE, light ? CaptureRequest.FLASH_MODE_TORCH : CaptureRequest.FLASH_MODE_OFF);
            request.set(CaptureRequest.CONTROL_AF_TRIGGER, CaptureRequest.CONTROL_AF_TRIGGER_IDLE);
            running.setRepeatingRequest(request.build(), watch, camera);
        } catch (Exception ignored) {

        }
    }

    private final CameraCaptureSession.CaptureCallback watch = new CameraCaptureSession.CaptureCallback() {
        @Override
        public void onCaptureCompleted(CameraCaptureSession s, CaptureRequest request, TotalCaptureResult result) {
            if (state != FOCUSING) {
                return;
            }
            Integer focus = result.get(CaptureResult.CONTROL_AF_STATE);
            boolean focused = !canFocus
                    || focus == null
                    || focus == CaptureResult.CONTROL_AF_STATE_FOCUSED_LOCKED
                    || focus == CaptureResult.CONTROL_AF_STATE_NOT_FOCUSED_LOCKED;
            long waited = System.currentTimeMillis() - focusStarted;
            if ((focused && settled()) || waited > FOCUS_MS) {
                takeStill();
            }
        }
    };

    private void stopCamera() {
        if (session != null) {
            session.close();
            session = null;
        }
        previewRequest = null;
        if (device != null) {
            device.close();
            device = null;
        }
        if (frames != null) {
            frames.close();
            frames = null;
        }
        if (photos != null) {
            photos.close();
            photos = null;
        }
        if (cameraThread != null) {
            cameraThread.quitSafely();
            cameraThread = null;
        }
        if (lookThread != null) {
            lookThread.quitSafely();
            lookThread = null;
        }
        state = PREVIEW;
    }

    private void fitPreview() {
        if (frameSize == null) {
            return;
        }
        runOnUiThread(() -> {
            float viewW = preview.getWidth();
            float viewH = preview.getHeight();
            if (viewW == 0 || viewH == 0) {
                return;
            }

            boolean turned = sensorTurn % 180 != 0;
            float picW = turned ? frameSize.getHeight() : frameSize.getWidth();
            float picH = turned ? frameSize.getWidth() : frameSize.getHeight();
            float scale = Math.max(viewW / picW, viewH / picH);

            Matrix fit = new Matrix();
            fit.setScale(picW * scale / viewW, picH * scale / viewH, viewW / 2, viewH / 2);
            preview.setTransform(fit);
            sheet.picture(picW, picH, scale, viewW, viewH);
        });
    }

    private void lookForTheSheet(ImageReader reader) {
        Image frame = reader.acquireLatestImage();
        if (frame == null) {
            return;
        }
        int[] argb;
        int uw;
        int uh;
        try {
            long now = System.currentTimeMillis();
            if (looking || now - lastLook < 100 || look == null) {
                return;
            }
            lastLook = now;
            int w = frame.getWidth();
            int h = frame.getHeight();
            Image.Plane[] planes = frame.getPlanes();
            byte[] ys = bytesOf(planes[0].getBuffer());
            byte[] us = bytesOf(planes[1].getBuffer());
            byte[] vs = bytesOf(planes[2].getBuffer());
            int yStride = planes[0].getRowStride();
            int uvStride = planes[1].getRowStride();
            int uvStep = planes[1].getPixelStride();
            boolean turned = sensorTurn % 180 != 0;
            uw = (turned ? h : w) / 2;
            uh = (turned ? w : h) / 2;
            argb = new int[uw * uh];
            for (int uy = 0; uy < uh; uy++) {
                for (int ux = 0; ux < uw; ux++) {
                    int fx = ux * 2;
                    int fy = uy * 2;
                    int x;
                    int y;
                    switch (sensorTurn) {
                        case 90:
                            x = fy;
                            y = h - 1 - fx;
                            break;
                        case 180:
                            x = w - 1 - fx;
                            y = h - 1 - fy;
                            break;
                        case 270:
                            x = w - 1 - fy;
                            y = fx;
                            break;
                        default:
                            x = fx;
                            y = fy;
                    }
                    x = Math.max(0, Math.min(w - 1, x));
                    y = Math.max(0, Math.min(h - 1, y));
                    int luma = ys[Math.min(ys.length - 1, y * yStride + x)] & 0xFF;
                    int at = Math.min(us.length - 1, (y / 2) * uvStride + (x / 2) * uvStep);
                    int u = (us[at] & 0xFF) - 128;
                    int v = (vs[Math.min(vs.length - 1, at)] & 0xFF) - 128;
                    int r = clamp(luma + (int) (1.402f * v));
                    int g = clamp(luma - (int) (0.344f * u + 0.714f * v));
                    int b = clamp(luma + (int) (1.772f * u));
                    argb[uy * uw + ux] = 0xFF000000 | (r << 16) | (g << 8) | b;
                }
            }
        } finally {
            frame.close();
        }
        looking = true;
        final int[] picture = argb;
        final int pw = uw;
        final int ph = uh;
        final int turns = nextTurn;
        nextTurn = (nextTurn + 1) % 4;
        look.post(() -> {
            float[] answer = lookForSheet(picture, pw, ph, turns);
            looking = false;
            long now = System.currentTimeMillis();
            if (answer != null) {
                float[] corners = new float[8];
                for (int i = 0; i < 8; i++) {
                    corners[i] = answer[i] * 2;
                }
                byTurn[turns] = clockwiseFromTopLeft(corners);
                sureness[turns] = answer[8];
            } else {
                byTurn[turns] = null;
                sureness[turns] = 0;
            }
            lookedAt[turns] = now;

            float[] best = null;
            float surest = SURE;
            for (int k = 0; k < 4; k++) {
                if (byTurn[k] != null && now - lookedAt[k] < RECENT_MS && sureness[k] >= surest) {
                    best = byTurn[k];
                    surest = sureness[k];
                }
            }
            sawSheet(best);
        });
    }

    private void sawSheet(float[] found) {
        long now = System.currentTimeMillis();
        boolean turned = sensorTurn % 180 != 0;
        float diagonal = frameSize == null ? 1
                : (float) Math.hypot(frameSize.getWidth(), frameSize.getHeight());
        if (found == null) {
            misses++;

            if (now - lastSeen > GONE_MS) {
                armed = true;
            }

            if (misses > 3) {
                recent.clear();
                lastShown = null;
                steadySince = 0;
                sheet.found(null, 0);
                hint(onCameraScreen ? "Looking for the page" : "");
            }
            return;
        }
        misses = 0;
        lastSeen = now;
        float[] latest = recent.peekLast();
        if (latest != null && farthest(latest, found) > 0.08f * diagonal) {

            recent.clear();
        }
        recent.addLast(found);
        while (recent.size() > 4) {
            recent.removeFirst();
        }
        float[] mean = new float[8];
        for (float[] quad : recent) {
            for (int i = 0; i < 8; i++) {
                mean[i] += quad[i] / recent.size();
            }
        }
        if (!armed && takenAt != null && farthest(takenAt, mean) > MOVED_ON * diagonal) {

            armed = true;
        }
        boolean still = lastShown != null && farthest(lastShown, mean) < STILL * diagonal && settled();
        if (!still) {
            steadySince = 0;
        } else if (steadySince == 0) {
            steadySince = now;
        }
        lastShown = mean;
        float held = steadySince == 0 ? 0 : Math.min(1f, (now - steadySince) / (float) STEADY_MS);
        sheet.found(mean, auto && armed ? held : 0);
        if (!onCameraScreen) {
            return;
        }
        hint(auto && armed ? "Hold still" : "");
        if (auto && armed && held >= 1f && state == PREVIEW) {
            runOnUiThread(this::takePicture);
        }
    }

    private boolean settled() {
        return System.currentTimeMillis() - lastShake > SETTLED_MS;
    }

    private void takePicture() {
        if (device == null || session == null || previewRequest == null || state != PREVIEW) {
            return;
        }
        armed = false;
        takenAt = lastShown;
        state = FOCUSING;
        focusStarted = System.currentTimeMillis();
        hint("Hold still");
        try {
            if (canFocus) {
                previewRequest.set(CaptureRequest.CONTROL_AF_TRIGGER, CaptureRequest.CONTROL_AF_TRIGGER_START);
                session.capture(previewRequest.build(), watch, camera);
                previewRequest.set(CaptureRequest.CONTROL_AF_TRIGGER, CaptureRequest.CONTROL_AF_TRIGGER_IDLE);
            }
        } catch (Exception failure) {

            Log.w("PanPDF", "scan: focus trigger failed", failure);
        }
    }

    private void takeStill() {
        if (state != FOCUSING || device == null || session == null) {
            return;
        }
        state = TAKING;
        runOnUiThread(() -> {
            hint("");
            if (finding == null) {
                finding = working("Finding the page\u2026");
                root.addView(finding, new FrameLayout.LayoutParams(-1, -1));
            }
        });
        try {
            CaptureRequest.Builder request = device.createCaptureRequest(CameraDevice.TEMPLATE_STILL_CAPTURE);
            request.addTarget(photos.getSurface());
            request.set(CaptureRequest.CONTROL_AF_MODE, CaptureRequest.CONTROL_AF_MODE_CONTINUOUS_PICTURE);
            request.set(CaptureRequest.CONTROL_AE_MODE, CaptureRequest.CONTROL_AE_MODE_ON);
            request.set(CaptureRequest.FLASH_MODE, light ? CaptureRequest.FLASH_MODE_TORCH : CaptureRequest.FLASH_MODE_OFF);
            request.set(CaptureRequest.JPEG_QUALITY, (byte) 95);
            session.capture(request.build(), new CameraCaptureSession.CaptureCallback() {
                @Override
                public void onCaptureCompleted(CameraCaptureSession s, CaptureRequest r, TotalCaptureResult result) {
                    letFocusGo();
                }
            }, camera);
        } catch (Exception failure) {
            Log.w("PanPDF", "scan: the picture was not taken", failure);
            state = PREVIEW;
            runOnUiThread(this::stopFinding);
            letFocusGo();
        }
    }

    private void letFocusGo() {
        if (session == null || previewRequest == null) {
            return;
        }
        try {
            if (canFocus) {
                previewRequest.set(CaptureRequest.CONTROL_AF_TRIGGER, CaptureRequest.CONTROL_AF_TRIGGER_CANCEL);
                session.capture(previewRequest.build(), null, camera);
            }
        } catch (Exception ignored) {

        }
        repeatPreview();
    }

    private void photoTaken(ImageReader reader) {
        Image image = reader.acquireLatestImage();
        if (image == null) {
            return;
        }
        byte[] jpeg;
        try {
            ByteBuffer buffer = image.getPlanes()[0].getBuffer();
            jpeg = new byte[buffer.remaining()];
            buffer.get(jpeg);
        } finally {
            image.close();
        }
        Bitmap taken = BitmapFactory.decodeByteArray(jpeg, 0, jpeg.length);
        if (taken == null) {
            state = PREVIEW;
            runOnUiThread(this::stopFinding);
            return;
        }
        if (sensorTurn != 0) {
            Matrix turn = new Matrix();
            turn.postRotate(sensorTurn);
            taken = Bitmap.createBitmap(taken, 0, 0, taken.getWidth(), taken.getHeight(), turn, true);
        }

        float[] corners = cornersOf(taken);
        Bitmap picture = taken;
        runOnUiThread(() -> review(picture, corners));
    }

    private float[] cornersOf(Bitmap picture) {
        int w = picture.getWidth();
        int h = picture.getHeight();
        float scale = Math.min(1f, LOOKED_AT / (float) Math.max(w, h));
        int sw = Math.max(1, Math.round(w * scale));
        int sh = Math.max(1, Math.round(h * scale));
        Bitmap medium = scale < 1f ? Bitmap.createScaledBitmap(picture, sw, sh, true) : picture;
        int[] argb = new int[sw * sh];
        medium.getPixels(argb, 0, sw, 0, 0, sw, sh);
        float[] found = cornersInPhoto(argb, sw, sh);
        if (found != null) {
            for (int i = 0; i < found.length; i++) {
                found[i] *= (i % 2 == 0) ? (float) w / sw : (float) h / sh;
            }
            return clockwiseFromTopLeft(found);
        }
        float mx = w * 0.08f;
        float my = h * 0.08f;
        return new float[] {mx, my, w - mx, my, w - mx, h - my, mx, h - my};
    }

    private void review(Bitmap picture, float[] corners) {
        stopFinding();
        onCameraScreen = false;
        hint("");
        FrameLayout screen = new FrameLayout(this);
        screen.setBackgroundColor(Color.BLACK);
        CornersView view = new CornersView(this, picture, corners);
        screen.addView(view, new FrameLayout.LayoutParams(-1, -1));
        LinearLayout bar = new LinearLayout(this);
        bar.setGravity(Gravity.CENTER);
        bar.setPadding(dp(16), dp(12), dp(16), dp(28));
        Button again = flatButton("Retake");
        again.setBackgroundColor(Color.TRANSPARENT);
        Button keep = flatButton("Keep");
        GradientDrawable blue = new GradientDrawable();
        blue.setColor(BLUE);
        blue.setCornerRadius(dp(26));
        keep.setBackground(blue);
        bar.addView(again, new LinearLayout.LayoutParams(0, dp(52), 1));
        bar.addView(new View(this), new LinearLayout.LayoutParams(dp(16), 1));
        bar.addView(keep, new LinearLayout.LayoutParams(0, dp(52), 1));
        screen.addView(bar, new FrameLayout.LayoutParams(-1, -2, Gravity.BOTTOM));
        screen.setOnApplyWindowInsetsListener((v, insets) -> {
            bar.setPadding(dp(16), dp(12), dp(16), dp(28) + insets.getSystemWindowInsetBottom());
            return insets;
        });
        setContentView(screen);
        again.setOnClickListener(v -> backToCamera());
        keep.setOnClickListener(v -> {
            keep.setEnabled(false);
            again.setEnabled(false);

            screen.addView(working("Straightening the page\u2026"), new FrameLayout.LayoutParams(-1, -1));
            float[] chosen = view.corners();
            new Thread(() -> {
                String path = flattenAndSave(picture, chosen);
                runOnUiThread(() -> {
                    if (path != null) {
                        pages.add(path);
                    }
                    showCount();
                    backToCamera();
                });
            }).start();
        });
    }

    private void backToCamera() {

        recent.clear();
        lastShown = null;
        Arrays.fill(byTurn, null);
        sheet.found(null, 0);
        setContentView(root);
        onCameraScreen = true;
        steadySince = 0;
        state = PREVIEW;
    }

    private void stopFinding() {
        if (finding != null) {
            root.removeView(finding);
            finding = null;
        }
    }

    private View working(String what) {
        LinearLayout cover = new LinearLayout(this);
        cover.setOrientation(LinearLayout.VERTICAL);
        cover.setGravity(Gravity.CENTER);
        cover.setBackgroundColor(Color.argb(150, 0, 0, 0));
        cover.setClickable(true);
        ProgressBar spinner = new ProgressBar(this);
        spinner.setIndeterminate(true);
        cover.addView(spinner, new LinearLayout.LayoutParams(dp(56), dp(56)));
        TextView said = new TextView(this);
        said.setText(what);
        said.setTextColor(Color.WHITE);
        said.setTextSize(16);
        said.setPadding(0, dp(16), 0, 0);
        cover.addView(said, new LinearLayout.LayoutParams(-2, -2));
        return cover;
    }

    private String flattenAndSave(Bitmap picture, float[] corners) {
        try {
            int w = picture.getWidth();
            int h = picture.getHeight();
            int[] argb = new int[w * h];
            picture.getPixels(argb, 0, w, 0, 0, w, h);
            int[] flat = flattenSheet(argb, w, h, corners, false);
            if (flat == null || flat.length < 2) {
                return null;
            }
            int fw = flat[0];
            int fh = flat[1];
            Bitmap page = Bitmap.createBitmap(flat, 2, fw, fw, fh, Bitmap.Config.ARGB_8888);
            File folder = new File(getCacheDir(), "scanned");
            folder.mkdirs();
            File file = new File(folder, "page-" + System.currentTimeMillis() + ".jpg");
            try (FileOutputStream out = new FileOutputStream(file)) {
                page.compress(Bitmap.CompressFormat.JPEG, 88, out);
            }
            return file.getAbsolutePath();
        } catch (Exception failure) {
            return null;
        }
    }

    private void showCount() {
        int n = pages.size();
        done.setText("Done · " + n);
        done.setVisibility(n > 0 ? View.VISIBLE : View.GONE);
    }

    private void showToggles() {
        styleToggle(autoButton, auto);
        styleToggle(lightButton, light);
        lightButton.setVisibility(hasLight ? View.VISIBLE : View.GONE);
    }

    private void styleToggle(Button button, boolean on) {
        GradientDrawable shape = new GradientDrawable();
        shape.setCornerRadius(dp(20));
        shape.setColor(on ? Color.WHITE : Color.argb(110, 0, 0, 0));
        shape.setStroke(dp(1), Color.argb(160, 255, 255, 255));
        button.setBackground(shape);
        button.setTextColor(on ? Color.BLACK : Color.WHITE);
    }

    private void hint(String text) {
        runOnUiThread(() -> {
            if (!text.contentEquals(hint.getText())) {
                hint.setText(text);
            }
        });
    }

    private void finishWithPages(boolean keep) {
        Intent result = new Intent();
        result.putExtra(PAGES, keep ? pages.toArray(new String[0]) : new String[0]);
        setResult(keep && !pages.isEmpty() ? RESULT_OK : RESULT_CANCELED, result);
        finish();
    }

    private Button flatButton(String text) {
        Button button = new Button(this);
        button.setText(text);
        button.setAllCaps(false);
        button.setTextColor(Color.WHITE);
        button.setTextSize(17);
        button.setBackgroundColor(Color.argb(90, 255, 255, 255));
        return button;
    }

    private Button pill(String text) {
        Button button = new Button(this);
        button.setText(text);
        button.setAllCaps(false);
        button.setTextSize(14);
        button.setMinWidth(0);
        button.setMinimumWidth(0);
        button.setPadding(dp(16), 0, dp(16), 0);
        return button;
    }

    private int dp(int value) {
        return Math.round(value * getResources().getDisplayMetrics().density);
    }

    private static int clamp(int v) {
        return v < 0 ? 0 : Math.min(255, v);
    }

    private static byte[] bytesOf(ByteBuffer buffer) {
        buffer.rewind();
        byte[] out = new byte[buffer.remaining()];
        buffer.get(out);
        return out;
    }

    private static float farthest(float[] a, float[] b) {
        float most = 0;
        for (int i = 0; i < 8; i += 2) {
            most = Math.max(most, (float) Math.hypot(a[i] - b[i], a[i + 1] - b[i + 1]));
        }
        return most;
    }

    private static Size largest(Size[] sizes) {
        Size best = sizes[0];
        for (Size size : sizes) {
            if ((long) size.getWidth() * size.getHeight() > (long) best.getWidth() * best.getHeight()) {
                best = size;
            }
        }
        return best;
    }

    private static Size near(Size[] sizes, int w, int h, Size shape) {
        float wanted = (float) shape.getWidth() / shape.getHeight();
        Size best = sizes[0];
        float bestScore = Float.MAX_VALUE;
        for (Size size : sizes) {
            float ratio = (float) size.getWidth() / size.getHeight();
            float score = Math.abs(ratio - wanted) * 4000 + Math.abs(size.getWidth() - w) + Math.abs(size.getHeight() - h);
            if (score < bestScore) {
                bestScore = score;
                best = size;
            }
        }
        return best;
    }

    static float[] clockwiseFromTopLeft(float[] p) {
        PointF[] points = new PointF[4];
        for (int i = 0; i < 4; i++) {
            points[i] = new PointF(p[2 * i], p[2 * i + 1]);
        }
        PointF tl = points[0];
        PointF tr = points[0];
        PointF br = points[0];
        PointF bl = points[0];
        for (PointF q : points) {
            if (q.x + q.y < tl.x + tl.y) {
                tl = q;
            }
            if (q.x + q.y > br.x + br.y) {
                br = q;
            }
            if (q.x - q.y > tr.x - tr.y) {
                tr = q;
            }
            if (q.y - q.x > bl.y - bl.x) {
                bl = q;
            }
        }
        return new float[] {tl.x, tl.y, tr.x, tr.y, br.x, br.y, bl.x, bl.y};
    }

    private static final class SheetView extends View {
        private float[] corners;
        private float held;
        private float picW = 1;
        private float picH = 1;
        private float scale = 1;
        private float viewW;
        private float viewH;
        private final Paint line = new Paint(Paint.ANTI_ALIAS_FLAG);
        private final Paint fill = new Paint(Paint.ANTI_ALIAS_FLAG);

        SheetView(Context context) {
            super(context);
            line.setStyle(Paint.Style.STROKE);
            line.setStrokeWidth(context.getResources().getDisplayMetrics().density * 3);
            line.setStrokeJoin(Paint.Join.ROUND);
        }

        void picture(float w, float h, float s, float vw, float vh) {
            picW = w;
            picH = h;
            scale = s;
            viewW = vw;
            viewH = vh;
        }

        void found(float[] points, float heldFor) {
            corners = points;
            held = heldFor;
            postInvalidate();
        }

        @Override
        protected void onDraw(Canvas canvas) {
            float[] points = corners;
            if (points == null) {
                return;
            }
            int colour = mix(BLUE, GREEN, held);
            line.setColor(colour);
            fill.setColor(Color.argb(60 + (int) (40 * held), Color.red(colour), Color.green(colour), Color.blue(colour)));
            float ox = (viewW - picW * scale) / 2;
            float oy = (viewH - picH * scale) / 2;
            Path path = new Path();
            for (int i = 0; i < 4; i++) {
                float x = ox + points[2 * i] * scale;
                float y = oy + points[2 * i + 1] * scale;
                if (i == 0) {
                    path.moveTo(x, y);
                } else {
                    path.lineTo(x, y);
                }
            }
            path.close();
            canvas.drawPath(path, fill);
            canvas.drawPath(path, line);
        }

        private static int mix(int a, int b, float t) {
            return Color.rgb(
                    (int) (Color.red(a) + (Color.red(b) - Color.red(a)) * t),
                    (int) (Color.green(a) + (Color.green(b) - Color.green(a)) * t),
                    (int) (Color.blue(a) + (Color.blue(b) - Color.blue(a)) * t));
        }
    }

    private static final class CornersView extends View {
        private final Bitmap picture;
        private final float[] corners;
        private final Paint line = new Paint(Paint.ANTI_ALIAS_FLAG);
        private final Paint grip = new Paint(Paint.ANTI_ALIAS_FLAG);
        private final Paint ring = new Paint(Paint.ANTI_ALIAS_FLAG);
        private final Matrix shown = new Matrix();
        private final Matrix back = new Matrix();
        private int held = -1;

        CornersView(Context context, Bitmap picture, float[] corners) {
            super(context);
            this.picture = picture;
            this.corners = corners.clone();
            float density = context.getResources().getDisplayMetrics().density;
            line.setColor(BLUE);
            line.setStyle(Paint.Style.STROKE);
            line.setStrokeWidth(density * 3);
            grip.setColor(Color.WHITE);
            ring.setColor(BLUE);
            ring.setStyle(Paint.Style.STROKE);
            ring.setStrokeWidth(density * 3);
        }

        float[] corners() {
            return corners.clone();
        }

        @Override
        protected void onSizeChanged(int w, int h, int oldW, int oldH) {
            float margin = getResources().getDisplayMetrics().density * 24;
            float room = h - margin * 5;
            float scale = Math.min((w - 2 * margin) / picture.getWidth(), room / picture.getHeight());
            shown.reset();
            shown.postScale(scale, scale);
            shown.postTranslate((w - picture.getWidth() * scale) / 2, margin + (room - picture.getHeight() * scale) / 2);
            shown.invert(back);
        }

        @Override
        protected void onDraw(Canvas canvas) {
            canvas.drawBitmap(picture, shown, null);
            float[] on = corners.clone();
            shown.mapPoints(on);
            Path path = new Path();
            path.moveTo(on[0], on[1]);
            for (int i = 1; i < 4; i++) {
                path.lineTo(on[2 * i], on[2 * i + 1]);
            }
            path.close();
            canvas.drawPath(path, line);
            float r = getResources().getDisplayMetrics().density * 12;
            for (int i = 0; i < 4; i++) {
                canvas.drawCircle(on[2 * i], on[2 * i + 1], r, grip);
                canvas.drawCircle(on[2 * i], on[2 * i + 1], r, ring);
            }
        }

        @Override
        public boolean onTouchEvent(MotionEvent event) {
            float[] at = {event.getX(), event.getY()};
            back.mapPoints(at);
            switch (event.getActionMasked()) {
                case MotionEvent.ACTION_DOWN:
                    held = -1;
                    float best = Float.MAX_VALUE;
                    float[] on = corners.clone();
                    shown.mapPoints(on);
                    float reach = getResources().getDisplayMetrics().density * 48;
                    for (int i = 0; i < 4; i++) {
                        float d = (float) Math.hypot(on[2 * i] - event.getX(), on[2 * i + 1] - event.getY());
                        if (d < best && d < reach) {
                            best = d;
                            held = i;
                        }
                    }
                    return held >= 0;
                case MotionEvent.ACTION_MOVE:
                    if (held >= 0) {
                        corners[2 * held] = Math.max(0, Math.min(picture.getWidth(), at[0]));
                        corners[2 * held + 1] = Math.max(0, Math.min(picture.getHeight(), at[1]));
                        invalidate();
                    }
                    return true;
                default:
                    held = -1;
                    return true;
            }
        }
    }
}
