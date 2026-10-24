package org.webrtc;

import android.os.HandlerThread;

public class EglThread {
    public static HandlerThread create() { return new HandlerThread("EglThread"); }
}
