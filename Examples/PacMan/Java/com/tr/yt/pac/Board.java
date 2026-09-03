package com.tr.yt.pac;

import android.content.Context;
import android.graphics.Canvas;
import android.graphics.Color;
import android.graphics.Paint;
import android.graphics.RectF;
import android.view.MotionEvent;
import android.view.View;

public final class Board extends View {

    private static final long STEP_MILLIS = 140L;

    private final Game game;
    private final Paint paint;
    private final RectF box;
    private long last;
    private float downX;
    private float downY;
    private int mouth;

    public Board(Context context) {
        super(context);
        game = new Game();
        paint = new Paint(Paint.ANTI_ALIAS_FLAG);
        box = new RectF();
        last = System.currentTimeMillis();
        setBackgroundColor(0xFF000000);
    }

    public Game game() {
        return game;
    }

    @Override
    protected void onDraw(Canvas canvas) {
        long now = System.currentTimeMillis();
        if (now - last >= STEP_MILLIS) {
            last = now;
            mouth = mouth + 1;
            game.tick();
        }

        Maze maze = game.maze();
        float side = Math.min(getWidth() / (float) maze.wide(),
                              getHeight() / (float) (maze.tall() + 3));
        float left = (getWidth() - side * maze.wide()) / 2f;
        float top = side * 2f;

        for (int row = 0; row < maze.tall(); row = row + 1) {
            for (int column = 0; column < maze.wide(); column = column + 1) {
                float x = left + column * side;
                float y = top + row * side;
                int held = maze.at(column, row);
                if (held == Maze.WALL) {
                    paint.setColor(0xFF1D2FA8);
                    box.set(x + side * 0.1f, y + side * 0.1f,
                            x + side * 0.9f, y + side * 0.9f);
                    canvas.drawRoundRect(box, side * 0.3f, side * 0.3f, paint);
                } else if (held == Maze.PELLET) {
                    paint.setColor(0xFFFFE0A0);
                    canvas.drawCircle(x + side / 2f, y + side / 2f, side * 0.10f, paint);
                } else if (held == Maze.POWER) {
                    paint.setColor(0xFFFFE0A0);
                    canvas.drawCircle(x + side / 2f, y + side / 2f, side * 0.28f, paint);
                }
            }
        }

        Ghost[] ghosts = game.ghosts();
        for (int index = 0; index < ghosts.length; index = index + 1) {
            Ghost ghost = ghosts[index];
            float x = left + ghost.column() * side + side / 2f;
            float y = top + ghost.row() * side + side / 2f;
            if (ghost.mood() == Ghost.EATEN) {
                paint.setColor(0xFF444466);
            } else if (ghost.edible()) {
                paint.setColor(0xFF3355FF);
            } else {
                paint.setColor(ghost.colour());
            }
            box.set(x - side * 0.42f, y - side * 0.42f, x + side * 0.42f, y + side * 0.42f);
            canvas.drawArc(box, 180f, 180f, true, paint);
            canvas.drawRect(x - side * 0.42f, y, x + side * 0.42f, y + side * 0.34f, paint);
            paint.setColor(Color.WHITE);
            canvas.drawCircle(x - side * 0.15f, y - side * 0.05f, side * 0.10f, paint);
            canvas.drawCircle(x + side * 0.15f, y - side * 0.05f, side * 0.10f, paint);
        }

        float px = left + game.column() * side + side / 2f;
        float py = top + game.row() * side + side / 2f;
        paint.setColor(0xFFFFE93B);
        box.set(px - side * 0.45f, py - side * 0.45f, px + side * 0.45f, py + side * 0.45f);
        float open = (mouth % 2 == 0) ? 50f : 12f;
        float start = game.facing() * 90f - 90f + open / 2f;
        canvas.drawArc(box, start, 360f - open, true, paint);

        paint.setColor(0xFFFFFFFF);
        paint.setTextSize(side * 1.2f);
        canvas.drawText("SCORE " + game.score(), left, side * 1.4f, paint);
        canvas.drawText("LIVES " + game.lives(), left + side * 16f, side * 1.4f, paint);

        if (game.state() != Game.PLAYING) {
            paint.setTextSize(side * 2f);
            String said = game.state() == Game.WON ? "CLEARED" : "GAME OVER";
            canvas.drawText(said, left + side * 6f, top + side * 13f, paint);
        }

        postInvalidateOnAnimation();
    }

    @Override
    public boolean onTouchEvent(MotionEvent event) {
        int action = event.getActionMasked();
        if (action == MotionEvent.ACTION_DOWN) {
            downX = event.getX();
            downY = event.getY();
            return true;
        }
        if (action != MotionEvent.ACTION_UP) {
            return true;
        }
        float dx = event.getX() - downX;
        float dy = event.getY() - downY;
        if (Math.abs(dx) < 24f && Math.abs(dy) < 24f) {
            return true;
        }
        if (Math.abs(dx) > Math.abs(dy)) {
            game.steer(dx > 0 ? Direction.RIGHT : Direction.LEFT);
        } else {
            game.steer(dy > 0 ? Direction.DOWN : Direction.UP);
        }
        return true;
    }
}
