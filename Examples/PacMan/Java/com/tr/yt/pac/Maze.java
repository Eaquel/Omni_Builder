package com.tr.yt.pac;

public final class Maze {

    public static final int WALL = 0;
    public static final int PELLET = 1;
    public static final int EMPTY = 2;
    public static final int POWER = 3;

    private static final String[] PLAN = {
        "###########################",
        "#............#............#",
        "#.####.#####.#.#####.####.#",
        "#o####.#####.#.#####.####o#",
        "#.####.#####.#.#####.####.#",
        "#.........................#",
        "#.####.##.#######.##.####.#",
        "#.####.##.#######.##.####.#",
        "#......##....#....##......#",
        "######.##### # #####.######",
        "     #.##          ##.#    ",
        "     #.## ###--### ##.#    ",
        "######.## #      # ##.######",
        "      .   #      #   .     ",
        "######.## #      # ##.######",
        "     #.## ######## ##.#    ",
        "     #.##          ##.#    ",
        "######.## ######## ##.######",
        "#............#............#",
        "#.####.#####.#.#####.####.#",
        "#o..##.......P.......##..o#",
        "###.##.##.#######.##.##.###",
        "#......##....#....##......#",
        "#.##########.#.##########.#",
        "#.........................#",
        "###########################",
    };

    private final int[][] cells;
    private final int wide;
    private final int tall;
    private int pellets;

    public Maze() {
        tall = PLAN.length;
        wide = PLAN[0].length();
        cells = new int[tall][wide];
        int counted = 0;
        for (int row = 0; row < tall; row = row + 1) {
            String line = PLAN[row];
            for (int column = 0; column < wide; column = column + 1) {
                char written = column < line.length() ? line.charAt(column) : ' ';
                int held;
                if (written == '#' || written == '-') {
                    held = WALL;
                } else if (written == '.') {
                    held = PELLET;
                    counted = counted + 1;
                } else if (written == 'o') {
                    held = POWER;
                    counted = counted + 1;
                } else {
                    held = EMPTY;
                }
                cells[row][column] = held;
            }
        }
        pellets = counted;
    }

    public int wide() {
        return wide;
    }

    public int tall() {
        return tall;
    }

    public int at(int column, int row) {
        if (row < 0 || row >= tall) {
            return WALL;
        }
        int wrapped = wrap(column);
        return cells[row][wrapped];
    }

    public int wrap(int column) {
        int held = column;
        while (held < 0) {
            held = held + wide;
        }
        while (held >= wide) {
            held = held - wide;
        }
        return held;
    }

    public boolean open(int column, int row) {
        return at(column, row) != WALL;
    }

    public int take(int column, int row) {
        if (row < 0 || row >= tall) {
            return EMPTY;
        }
        int wrapped = wrap(column);
        int held = cells[row][wrapped];
        if (held == PELLET || held == POWER) {
            cells[row][wrapped] = EMPTY;
            pellets = pellets - 1;
        }
        return held;
    }

    public int left() {
        return pellets;
    }

    public boolean cleared() {
        return pellets <= 0;
    }

    public int startColumn() {
        return findStart(true);
    }

    public int startRow() {
        return findStart(false);
    }

    private int findStart(boolean wantColumn) {
        for (int row = 0; row < tall; row = row + 1) {
            String line = PLAN[row];
            for (int column = 0; column < line.length(); column = column + 1) {
                if (line.charAt(column) == 'P') {
                    return wantColumn ? column : row;
                }
            }
        }
        return 1;
    }
}
