s = 0;
for k = 1:10
    if mod(k, 2) == 0
        continue
    end
    if k > 7
        break
    end
    s = s + k;
end
s
n = 0;
while n < 5
    n = n + 2;
end
n
for c = [1 2; 3 4]
    disp(c')
end
switch 'abc'
    case {'x', 'y'}
        disp('no')
    case 'abc'
        disp('matched text')
    otherwise
        disp('other')
end
