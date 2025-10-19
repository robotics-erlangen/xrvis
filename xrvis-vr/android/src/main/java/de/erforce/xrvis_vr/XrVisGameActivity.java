package de.erforce.xrvis_vr;

import com.google.androidgamesdk.GameActivity;

public class XrVisGameActivity extends GameActivity {
    static {
        System.loadLibrary("xrvis_vr");
    }
}
