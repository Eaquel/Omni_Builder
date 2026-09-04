package com.tr.yt.brick;

import android.app.Activity;
import android.content.Context;
import android.graphics.Canvas;
import android.graphics.Paint;
import android.graphics.RectF;
import android.os.Bundle;
import android.view.MotionEvent;
import android.view.View;

public final class MainActivity extends Activity {

    private Table table;

    @Override
    protected void onCreate(Bundle state) {
        super.onCreate(state);
        table = new Table(this);
        setContentView(table);
    }

    public Table table() {
        return table;
    }

    static final class Wall {

        static final int COLUMNS = 8;
        static final int ROWS = 5;

        private static final int[] SHADES = {
            0xFFEF476F,
            0xFFFFD166,
            0xFF06D6A0,
            0xFF118AB2,
            0xFF9B5DE5,
        };

        private final boolean[] standing;
        private int left;

        Wall() {
            standing = new boolean[COLUMNS * ROWS];
            build();
        }

        void build() {
            for (int at = 0; at < standing.length; at = at + 1) {
                standing[at] = true;
            }
            left = standing.length;
        }

        boolean standing(int column, int row) {
            return standing[row * COLUMNS + column];
        }

        void knock(int column, int row) {
            standing[row * COLUMNS + column] = false;
            left = left - 1;
        }

        int left() {
            return left;
        }

        static int shade(int row) {
            return SHADES[row % SHADES.length];
        }
    }

    static final class Table extends View {

        static final int WAITING = 0;
        static final int PLAYING = 1;
        static final int LOST = 2;
        static final int CLEARED = 3;

        private static final int STARTING_LIVES = 3;
        private static final int BRICK_SCORE = 10;
        private static final float LONGEST_STEP = 0.05f;
        private static final int SUB_STEPS = 4;

        private final Paint paint;
        private final RectF box;
        private final Wall wall;

        private float wide;
        private float tall;
        private float side;
        private float brickWide;
        private float brickTall;
        private float roof;
        private float batWide;
        private float batTall;
        private float batX;
        private float batY;
        private float ballX;
        private float ballY;
        private float ballR;
        private float goX;
        private float goY;

        private int score;
        private int lives;
        private int state;
        private long last;

        Table(Context context) {
            super(context);
            paint = new Paint(Paint.ANTI_ALIAS_FLAG);
            box = new RectF();
            wall = new Wall();
            lives = STARTING_LIVES;
            state = WAITING;
            last = System.currentTimeMillis();
            setBackgroundColor(0xFF0B1020);
        }

        int score() {
            return score;
        }

        int lives() {
            return lives;
        }

        int state() {
            return state;
        }

        int standing() {
            return wall.left();
        }

        void measure(float across, float down) {
            wide = across;
            tall = down;
            side = Math.min(across, down) / 40f;
            brickWide = across / Wall.COLUMNS;
            brickTall = side * 2.4f;
            roof = side * 7f;
            batWide = across / 4.5f;
            batTall = side * 1.3f;
            batY = down - side * 6f;
            batX = across / 2f;
            ballR = side * 1.1f;
            rest();
        }

        void rest() {
            ballX = batX;
            ballY = batY - ballR - side * 0.3f;
            goX = side * 13f;
            goY = -side * 26f;
        }

        void begin() {
            wall.build();
            score = 0;
            lives = STARTING_LIVES;
            state = WAITING;
            rest();
        }

        void advance(float seconds) {
            if (state != PLAYING) {
                return;
            }
            float slice = seconds / SUB_STEPS;
            for (int step = 0; step < SUB_STEPS && state == PLAYING; step = step + 1) {
                move(slice);
            }
        }

        private void move(float seconds) {
            ballX = ballX + goX * seconds;
            ballY = ballY + goY * seconds;

            if (ballX < ballR) {
                ballX = ballR;
                goX = -goX;
            }
            if (ballX > wide - ballR) {
                ballX = wide - ballR;
                goX = -goX;
            }
            if (ballY < ballR) {
                ballY = ballR;
                goY = -goY;
            }

            strikeWall();
            strikeBat();

            if (ballY - ballR > tall) {
                drop();
            }
        }

        private void strikeWall() {
            float floor = roof + Wall.ROWS * brickTall;
            if (ballY < roof || ballY > floor) {
                return;
            }
            int column = (int) (ballX / brickWide);
            int row = (int) ((ballY - roof) / brickTall);
            if (column < 0 || column >= Wall.COLUMNS || row < 0 || row >= Wall.ROWS) {
                return;
            }
            if (!wall.standing(column, row)) {
                return;
            }
            wall.knock(column, row);
            score = score + BRICK_SCORE;
            goY = -goY;
            if (wall.left() == 0) {
                state = CLEARED;
            }
        }

        private void strikeBat() {
            if (goY < 0f) {
                return;
            }
            if (ballY + ballR < batY || ballY - ballR > batY + batTall) {
                return;
            }
            float half = batWide / 2f;
            if (ballX < batX - half || ballX > batX + half) {
                return;
            }
            ballY = batY - ballR;
            goY = -goY;
            goX = goX + (ballX - batX) / half * side * 14f;
            float most = side * 30f;
            if (goX > most) {
                goX = most;
            }
            if (goX < -most) {
                goX = -most;
            }
        }

        private void drop() {
            lives = lives - 1;
            if (lives <= 0) {
                state = LOST;
            } else {
                state = WAITING;
            }
            rest();
        }

        void steer(float x) {
            float half = batWide / 2f;
            batX = x;
            if (batX < half) {
                batX = half;
            }
            if (batX > wide - half) {
                batX = wide - half;
            }
            if (state != PLAYING) {
                ballX = batX;
            }
        }

        void press() {
            if (state == WAITING) {
                state = PLAYING;
                last = System.currentTimeMillis();
            } else if (state == LOST || state == CLEARED) {
                begin();
            }
        }

        @Override
        protected void onDraw(Canvas canvas) {
            if (side <= 0f) {
                if (getWidth() <= 0 || getHeight() <= 0) {
                    postInvalidateOnAnimation();
                    return;
                }
                measure(getWidth(), getHeight());
            }

            long now = System.currentTimeMillis();
            float seconds = (now - last) / 1000f;
            last = now;
            if (seconds > LONGEST_STEP) {
                seconds = LONGEST_STEP;
            }
            if (seconds < 0f) {
                seconds = 0f;
            }
            advance(seconds);

            for (int row = 0; row < Wall.ROWS; row = row + 1) {
                for (int column = 0; column < Wall.COLUMNS; column = column + 1) {
                    if (!wall.standing(column, row)) {
                        continue;
                    }
                    float x = column * brickWide;
                    float y = roof + row * brickTall;
                    paint.setColor(Wall.shade(row));
                    box.set(x + side * 0.25f, y + side * 0.25f,
                            x + brickWide - side * 0.25f, y + brickTall - side * 0.25f);
                    canvas.drawRoundRect(box, side * 0.5f, side * 0.5f, paint);
                }
            }

            paint.setColor(0xFFE7ECF5);
            box.set(batX - batWide / 2f, batY, batX + batWide / 2f, batY + batTall);
            canvas.drawRoundRect(box, batTall / 2f, batTall / 2f, paint);

            paint.setColor(0xFFFFE93B);
            canvas.drawCircle(ballX, ballY, ballR, paint);

            paint.setColor(0xFFE7ECF5);
            paint.setTextSize(side * 2f);
            canvas.drawText("SCORE " + score, side * 1.5f, side * 3.6f, paint);
            canvas.drawText("LIVES " + lives, wide - side * 12f, side * 3.6f, paint);

            if (state != PLAYING) {
                paint.setTextSize(side * 2.6f);
                canvas.drawText(said(), side * 1.5f, tall / 2f, paint);
            }

            postInvalidateOnAnimation();
        }

        private String said() {
            if (state == CLEARED) {
                return "CLEARED - TAP TO PLAY AGAIN";
            }
            if (state == LOST) {
                return "GAME OVER - TAP TO PLAY AGAIN";
            }
            return "TAP TO SERVE";
        }

        @Override
        public boolean onTouchEvent(MotionEvent event) {
            if (side <= 0f) {
                return true;
            }
            int action = event.getActionMasked();
            if (action == MotionEvent.ACTION_DOWN || action == MotionEvent.ACTION_MOVE) {
                steer(event.getX());
                return true;
            }
            if (action == MotionEvent.ACTION_UP) {
                press();
            }
            return true;
        }
    }
}
