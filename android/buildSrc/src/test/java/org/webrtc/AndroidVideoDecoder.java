package org.webrtc;

/** Models the calls made for every hardware-decoded texture output. */
public class AndroidVideoDecoder {
    public void deliverTextureFrame(SurfaceTextureHelper helper, int width, int height, int rotation) {
        helper.setTextureSize(width, height);
        helper.setFrameRotation(rotation);
    }
}
