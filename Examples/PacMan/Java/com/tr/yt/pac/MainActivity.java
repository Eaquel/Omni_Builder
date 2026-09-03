package com.tr.yt.pac;

import android.app.Activity;
import android.os.Bundle;

public final class MainActivity extends Activity {

    private Board board;

    @Override
    protected void onCreate(Bundle state) {
        super.onCreate(state);
        board = new Board(this);
        setContentView(board);
    }
}
