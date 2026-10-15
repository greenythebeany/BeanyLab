try
    error('pkg:bad', 'Value %d is bad', 7);
catch err
    disp(err.identifier)
    disp(err.message)
end
try
    x = [1 2 3];
    x(5)
catch err
    disp(err.message)
end
undefinedThing + 1
