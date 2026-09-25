// a line comment
package fixture;

/* a block comment */
@Deprecated
public class Everything {
    private final Runnable handler = () -> {};

    static {
        init();
    }

    Everything(int a) {
        this.a = a;
    }

    interface Shape {
        double area();
    }

    enum Op {
        PLUS {
            int apply(int a, int b) { return a + b; }
        };

        abstract int apply(int a, int b);
    }

    record Point(int x, int y) {
        Point {
            if (x > y) throw new IllegalArgumentException();
        }
    }

    @interface Marker {}

    int everything(boolean a, boolean b, int[] xs) {
        if (a) {
            f();
        } else if (b) {
            g();
        } else {
            h();
        }
        int y = a ? 1 : 2;
        switch (y) {
            case 1 -> f();
            default -> g();
        }
        for (int i = 0; i < 3; i++) {
            f();
        }
        for (int x : xs) {
            break;
        }
        outer:
        while (a) {
            continue outer;
        }
        do {
            f();
        } while (b);
        try {
            f();
        } catch (RuntimeException e) {
            g();
        }
        new Thread(new Runnable() {
            public void run() {}
        });
        if (a && (b || a)) {
            f();
        }
        return everything(a, b, xs);
    }
}
