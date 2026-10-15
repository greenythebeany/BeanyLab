A = magic(4);
a = A(2, 3)
b = A(end, :)
c = A(:, end)'
d = A(end)
e = A([1 2], [1 end])
g = A(A > 12)'
