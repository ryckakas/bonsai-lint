// a comment
package fixture

var handler = func() int { return 1 }

type Stack struct{ items []int }

func (s *Stack) Push(v int) { s.items = append(s.items, v) }

func generic[T any](x T) { generic[T](x) }

func everything(a, b bool, c chan int, x interface{}) int {
	if a {
		f()
	} else if b {
		g()
	} else {
		h()
	}
	switch {
	case a:
		f()
	default:
		g()
	}
	switch x.(type) {
	case int:
		f()
	}
	select {
	case <-c:
		f()
	default:
	}
	for i := 0; i < 3; i++ {
		f()
	}
	for range c {
		break
	}
outer:
	for a {
		continue outer
	}
	goto done
done:
	if a && (b || a) {
		f()
	}
	return everything(a, b, c, x)
}
