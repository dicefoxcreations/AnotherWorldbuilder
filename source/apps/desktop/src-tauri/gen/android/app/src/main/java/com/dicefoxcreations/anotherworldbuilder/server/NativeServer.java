package com.dicefoxcreations.anotherworldbuilder.server;

final class NativeServer {
    static { System.loadLibrary("aw_desktop_lib"); }
    private NativeServer() {}
    static native String start(String config);
    static native void stop();
    static native String status();
    static native String version();
    static native String worlds(String data);
}
