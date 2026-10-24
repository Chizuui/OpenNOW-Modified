package com.opencloudgaming.buildlogic;

import com.android.build.api.instrumentation.AsmClassVisitorFactory;
import com.android.build.api.instrumentation.ClassContext;
import com.android.build.api.instrumentation.ClassData;
import com.android.build.api.instrumentation.InstrumentationParameters;
import org.objectweb.asm.ClassVisitor;

/** Applies the same pinned WebRTC texture adjustments to debug and release builds. */
public abstract class WebRtcTextureDeliveryFactory
        implements AsmClassVisitorFactory<InstrumentationParameters.None> {
    @Override
    public boolean isInstrumentable(ClassData data) {
        String name = data.getClassName().replace('.', '/');
        return name.equals(WebRtcTextureDelivery.TEXTURE_HELPER)
                || name.equals(WebRtcTextureDelivery.EGL_THREAD)
                || name.equals(WebRtcTextureDelivery.DECODER);
    }

    @Override
    public ClassVisitor createClassVisitor(ClassContext context, ClassVisitor next) {
        return new WebRtcTextureDelivery(next,
                context.getCurrentClassData().getClassName().replace('.', '/'));
    }
}
