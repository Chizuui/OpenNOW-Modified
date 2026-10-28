package com.opencloudgaming.buildlogic;

import android.os.HandlerThread;
import java.io.InputStream;
import java.lang.reflect.InvocationTargetException;
import java.util.List;
import org.junit.Before;
import org.junit.Test;
import org.objectweb.asm.ClassReader;
import org.objectweb.asm.ClassWriter;
import static org.junit.Assert.*;

public class WebRtcTextureDeliveryTest {
    private Class<?> type;
    private Object helper;
    private TunedClassLoader loader;

    private static class TunedClassLoader extends ClassLoader {
        TunedClassLoader(ClassLoader parent) { super(parent); }
        Class<?> defineTuned(String name, byte[] bytes) {
            return defineClass(name.replace('/', '.'), bytes, 0, bytes.length);
        }
    }

    private Class<?> instrument(String name) throws Exception {
        byte[] original;
        try (InputStream stream = getClass().getResourceAsStream("/" + name + ".class")) {
            original = stream.readAllBytes();
        }
        ClassWriter writer = new ClassWriter(ClassWriter.COMPUTE_FRAMES | ClassWriter.COMPUTE_MAXS);
        new ClassReader(original).accept(new WebRtcTextureDelivery(writer, name), 0);
        byte[] tuned = writer.toByteArray();
        return loader.defineTuned(name, tuned);
    }

    @Before
    public void setup() throws Exception {
        loader = new TunedClassLoader(getClass().getClassLoader());
        type = instrument(WebRtcTextureDelivery.TEXTURE_HELPER);
        helper = type.getConstructor().newInstance();
    }

    private void size(int width, int height) throws Exception {
        type.getMethod("opennowSetTextureSize", int.class, int.class).invoke(helper, width, height);
    }

    private void rotation(int rotation) throws Exception {
        type.getMethod("opennowSetFrameRotation", int.class).invoke(helper, rotation);
    }

    private Object requests() throws Exception { return type.getField("requests").get(helper); }

    @Test
    public void decoderActuallyUsesOptimizedEntryPoints() throws Exception {
        Class<?> decoder = instrument(WebRtcTextureDelivery.DECODER);
        Object instance = decoder.getConstructor().newInstance();
        for (int i = 0; i < 60; i++) {
            decoder.getMethod("deliverTextureFrame", type, int.class, int.class, int.class)
                    .invoke(instance, helper, 1376, 640, 0);
        }
        assertEquals(List.of("size:1376x640", "rotation:0"), requests());
    }

    @Test
    public void originalSettersRemainUnchangedForCameraAndOtherCallers() throws Exception {
        for (int i = 0; i < 2; i++) {
            type.getMethod("setTextureSize", int.class, int.class).invoke(helper, 1280, 720);
            type.getMethod("setFrameRotation", int.class).invoke(helper, 0);
        }
        assertEquals(List.of("size:1280x720", "rotation:0", "size:1280x720", "rotation:0"), requests());
    }

    @Test
    public void steadySixtyFpsPostsMetadataOnceEvenWhileWorkerIsDelayed() throws Exception {
        for (int i = 0; i < 60; i++) {
            size(1376, 640);
            rotation(0);
        }
        assertEquals(List.of("size:1376x640", "rotation:0"), requests());
        type.getMethod("drainWorker").invoke(helper);
        size(1376, 640);
        rotation(0);
        assertEquals(List.of(), requests());
        assertEquals(List.of("size:1376x640", "rotation:0"), type.getField("applied").get(helper));
    }

    @Test
    public void resolutionAndRotationTransitionsKeepTheirOrdering() throws Exception {
        size(1376, 640);
        rotation(0);
        size(1280, 720);
        rotation(90);
        size(1376, 640);
        rotation(0);
        assertEquals(List.of("size:1376x640", "rotation:0", "size:1280x720", "rotation:90",
                "size:1376x640", "rotation:0"), requests());
    }

    @Test
    public void eitherDimensionChangingRequiresAnUpdate() throws Exception {
        size(1280, 720);
        size(1280, 800);
        size(1440, 800);
        assertEquals(List.of("size:1280x720", "size:1280x800", "size:1440x800"), requests());
    }

    @Test
    public void newHelperAfterReconnectStillReceivesInitialMetadata() throws Exception {
        size(1376, 640);
        rotation(0);
        helper = type.getConstructor().newInstance();
        size(1376, 640);
        rotation(0);
        assertEquals(List.of("size:1376x640", "rotation:0"), requests());
    }

    @Test
    public void invalidSizesAreNeverCachedOrSilenced() throws Exception {
        for (int[] dimensions : new int[][] {{0, 0}, {0, 720}, {1280, -1}, {0, 0}}) {
            try {
                size(dimensions[0], dimensions[1]);
                fail("Expected upstream size validation");
            } catch (InvocationTargetException e) {
                assertTrue(e.getCause() instanceof IllegalArgumentException);
            }
        }
        size(1280, 720);
        assertEquals(List.of("size:1280x720"), requests());
    }

    @Test
    public void failedSizeRequestCanBeRetriedWithSameDimensions() throws Exception {
        type.getField("failSize").set(helper, true);
        try {
            size(1376, 640);
            fail("Expected the platform error");
        } catch (InvocationTargetException e) {
            assertTrue(e.getCause() instanceof IllegalStateException);
        }
        type.getField("failSize").set(helper, false);
        size(1376, 640);
        assertEquals(List.of("size:1376x640"), requests());
    }

    @Test
    public void decoderTextureThreadGetsDisplayPriorityWithoutChangingCaptureThreads() throws Exception {
        HandlerThread decoder = (HandlerThread) type.getMethod("create", String.class)
                .invoke(null, "decoder-texture-thread");
        HandlerThread camera = (HandlerThread) type.getMethod("create", String.class)
                .invoke(null, "camera-capture-thread");
        assertEquals(-4, decoder.priority);
        assertEquals("decoder-texture-thread", decoder.name);
        assertEquals(0, camera.priority);
    }

    @Test
    public void displayRendererGetsDisplayPriority() throws Exception {
        Class<?> egl = instrument(WebRtcTextureDelivery.EGL_THREAD);
        HandlerThread thread = (HandlerThread) egl.getMethod("create").invoke(null);
        assertEquals(-4, thread.priority);
        assertEquals("EglThread", thread.name);
    }

    @Test
    public void dependencyShapeChangesFailBuildInsteadOfSilentlySkippingTuning() {
        for (String owner : List.of(WebRtcTextureDelivery.TEXTURE_HELPER,
                WebRtcTextureDelivery.EGL_THREAD, WebRtcTextureDelivery.DECODER)) {
            try {
                new WebRtcTextureDelivery(new ClassWriter(0), owner).visitEnd();
                fail("Expected a pinned dependency mismatch");
            } catch (IllegalStateException e) {
                assertTrue(e.getMessage().contains("WebRTC texture API changed"));
            }
        }
    }
}
