a = 2;
f = @(x) a * x + 1;
a = 100;
y = f(3)
g = @(x, y) x + y;
z = g(1, 2)
h = @max;
k = h([3 9 1])
