package com.tr.yt.pac;

public final class Ghost {

    public static final int CHASING = 0;
    public static final int FRIGHTENED = 1;
    public static final int EATEN = 2;

    private final int homeColumn;
    private final int homeRow;
    private final int colour;
    private final int scatterColumn;
    private final int scatterRow;

    private int column;
    private int row;
    private int facing;
    private int mood;
    private int frightenedFor;
    private int seed;

    public Ghost(int column, int row, int colour, int scatterColumn, int scatterRow, int seed) {
        this.homeColumn = column;
        this.homeRow = row;
        this.column = column;
        this.row = row;
        this.colour = colour;
        this.scatterColumn = scatterColumn;
        this.scatterRow = scatterRow;
        this.facing = Direction.LEFT;
        this.mood = CHASING;
        this.seed = seed;
    }

    public int column() {
        return column;
    }

    public int row() {
        return row;
    }

    public int colour() {
        return colour;
    }

    public int mood() {
        return mood;
    }

    public boolean edible() {
        return mood == FRIGHTENED;
    }

    public void frighten(int ticks) {
        if (mood == EATEN) {
            return;
        }
        mood = FRIGHTENED;
        frightenedFor = ticks;
        facing = Direction.opposite(facing);
    }

    public void eaten() {
        mood = EATEN;
        frightenedFor = 0;
    }

    public void home() {
        column = homeColumn;
        row = homeRow;
        mood = CHASING;
        frightenedFor = 0;
    }

    public void step(Maze maze, int targetColumn, int targetRow, boolean scattering) {
        if (mood == FRIGHTENED) {
            frightenedFor = frightenedFor - 1;
            if (frightenedFor <= 0) {
                mood = CHASING;
            }
        }
        if (mood == EATEN && column == homeColumn && row == homeRow) {
            mood = CHASING;
        }

        int wantColumn;
        int wantRow;
        if (mood == EATEN) {
            wantColumn = homeColumn;
            wantRow = homeRow;
        } else if (mood == FRIGHTENED) {
            wantColumn = wander(true);
            wantRow = wander(false);
        } else if (scattering) {
            wantColumn = scatterColumn;
            wantRow = scatterRow;
        } else {
            wantColumn = targetColumn;
            wantRow = targetRow;
        }

        int best = -1;
        int shortest = Integer.MAX_VALUE;
        for (int way = 0; way < 4; way = way + 1) {
            if (way == Direction.opposite(facing) && hasChoice(maze)) {
                continue;
            }
            int nextColumn = maze.wrap(column + Direction.dx(way));
            int nextRow = row + Direction.dy(way);
            if (!maze.open(nextColumn, nextRow)) {
                continue;
            }
            int reach = far(nextColumn, nextRow, wantColumn, wantRow);
            if (reach < shortest) {
                shortest = reach;
                best = way;
            }
        }
        if (best < 0) {
            best = Direction.opposite(facing);
        }
        facing = best;
        int aheadColumn = maze.wrap(column + Direction.dx(facing));
        int aheadRow = row + Direction.dy(facing);
        if (maze.open(aheadColumn, aheadRow)) {
            column = aheadColumn;
            row = aheadRow;
        }
    }

    private boolean hasChoice(Maze maze) {
        int ways = 0;
        for (int way = 0; way < 4; way = way + 1) {
            if (maze.open(maze.wrap(column + Direction.dx(way)), row + Direction.dy(way))) {
                ways = ways + 1;
            }
        }
        return ways > 2;
    }

    private static int far(int fromColumn, int fromRow, int toColumn, int toRow) {
        int dx = fromColumn - toColumn;
        int dy = fromRow - toRow;
        return dx * dx + dy * dy;
    }

    private int wander(boolean wantColumn) {
        seed = seed * 1103515245 + 12345;
        int held = (seed >> 16) & 0x7fff;
        return wantColumn ? held % 27 : (held / 27) % 26;
    }
}
