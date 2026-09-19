<?php
#[Attr]
function everything($a, $b) {
    // a comment
    if ($a) { echo 1; } elseif ($b) { echo 2; } else { echo 3; }
    $ternary = $a ? 1 : 2;
    switch ($a) { case 1: echo 1; break; default: echo 2; }
    $matched = match ($a) { 1 => 'a', default => 'b' };
    for ($i = 0; $i < 3; $i++) { echo $i; }
    foreach ($a as $x) { echo $x; }
    while ($a) { break 2; }
    do { echo 1; } while ($a);
    try { everything(1, 2); } catch (Exception $e) { continue 2; }
    if ($a && ($b || $c)) { echo 1; }
    $closure = function () { return 1; };
    $arrow = fn () => 1;
    $this->everything(1, 2);
    self::everything(1, 2);
    goto finish;
    finish:
}

class Sample {
    public function method() { return 1; }
}

interface Contract {
    public function contract();
}

trait Helper {
    public function helped() { return 1; }
}

enum Suit {
    case Hearts;
}
