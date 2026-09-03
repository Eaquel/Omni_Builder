package com.tr.yt.pac;

public final class Game {

    public static final int PLAYING = 0;
    public static final int LOST = 1;
    public static final int WON = 2;

    private static final int FRIGHTENED_TICKS = 40;
    private static final int SCATTER_TICKS = 28;
    private static final int CHASE_TICKS = 90;
    private static final int PELLET_SCORE = 10;
    private static final int POWER_SCORE = 50;
    private static final int GHOST_SCORE = 200;

    private final Maze maze;
    private final Ghost[] ghosts;

    private int column;
    private int row;
    private int facing;
    private int wanted;
    private int score;
    private int lives;
    private int state;
    private int ticks;
    private int chain;

    public Game() {
        maze = new Maze();
        column = maze.startColumn();
        row = maze.startRow();
        facing = Direction.LEFT;
        wanted = Direction.LEFT;
        lives = 3;
        state = PLAYING;
        ghosts = new Ghost[] {
            new Ghost(13, 11, 0xFFFF0000, 25, 0, 7),
            new Ghost(13, 13, 0xFFFFB8FF, 2, 0, 11),
            new Ghost(12, 13, 0xFF00FFFF, 25, 25, 13),
            new Ghost(14, 13, 0xFFFFB851, 2, 25, 17),
        };
    }

    public Maze maze() {
        return maze;
    }

    public Ghost[] ghosts() {
        return ghosts;
    }

    public int column() {
        return column;
    }

    public int row() {
        return row;
    }

    public int facing() {
        return facing;
    }

    public int score() {
        return score;
    }

    public int lives() {
        return lives;
    }

    public int state() {
        return state;
    }

    public void steer(int way) {
        wanted = way;
    }

    public void tick() {
        if (state != PLAYING) {
            return;
        }
        ticks = ticks + 1;
        movePlayer();
        eat();
        if (state != PLAYING) {
            return;
        }
        boolean scattering = (ticks % (SCATTER_TICKS + CHASE_TICKS)) < SCATTER_TICKS;
        for (int index = 0; index < ghosts.length; index = index + 1) {
            ghosts[index].step(maze, column, row, scattering);
        }
        touch();
    }

    private void movePlayer() {
        int wantColumn = maze.wrap(column + Direction.dx(wanted));
        int wantRow = row + Direction.dy(wanted);
        if (maze.open(wantColumn, wantRow)) {
            facing = wanted;
        }
        int nextColumn = maze.wrap(column + Direction.dx(facing));
        int nextRow = row + Direction.dy(facing);
        if (maze.open(nextColumn, nextRow)) {
            column = nextColumn;
            row = nextRow;
        }
    }

    private void eat() {
        int held = maze.take(column, row);
        if (held == Maze.PELLET) {
            score = score + PELLET_SCORE;
        } else if (held == Maze.POWER) {
            score = score + POWER_SCORE;
            chain = 0;
            for (int index = 0; index < ghosts.length; index = index + 1) {
                ghosts[index].frighten(FRIGHTENED_TICKS);
            }
        }
        if (maze.cleared()) {
            state = WON;
        }
    }

    private void touch() {
        for (int index = 0; index < ghosts.length; index = index + 1) {
            Ghost ghost = ghosts[index];
            if (ghost.column() != column || ghost.row() != row) {
                continue;
            }
            if (ghost.edible()) {
                chain = chain + 1;
                score = score + GHOST_SCORE * chain;
                ghost.eaten();
            } else if (ghost.mood() != Ghost.EATEN) {
                lives = lives - 1;
                if (lives <= 0) {
                    state = LOST;
                    return;
                }
                restart();
                return;
            }
        }
    }

    private void restart() {
        column = maze.startColumn();
        row = maze.startRow();
        facing = Direction.LEFT;
        wanted = Direction.LEFT;
        for (int index = 0; index < ghosts.length; index = index + 1) {
            ghosts[index].home();
        }
    }
}
