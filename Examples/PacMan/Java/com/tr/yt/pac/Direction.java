package com.tr.yt.pac;

public final class Direction {

    public static final int UP = 0;
    public static final int RIGHT = 1;
    public static final int DOWN = 2;
    public static final int LEFT = 3;

    private Direction() {
    }

    public static int dx(int way) {
        if (way == RIGHT) {
            return 1;
        }
        if (way == LEFT) {
            return -1;
        }
        return 0;
    }

    public static int dy(int way) {
        if (way == DOWN) {
            return 1;
        }
        if (way == UP) {
            return -1;
        }
        return 0;
    }

    public static int opposite(int way) {
        return (way + 2) % 4;
    }
}
