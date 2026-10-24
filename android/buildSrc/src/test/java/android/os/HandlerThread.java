package android.os;

/** JVM fixture: records which Android constructor the instrumented factory selects. */
public class HandlerThread {
    public final String name;
    public final int priority;

    public HandlerThread(String name) { this(name, 0); }
    public HandlerThread(String name, int priority) {
        this.name = name;
        this.priority = priority;
    }
}
