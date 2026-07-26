// Minimal progressive enhancement: sticky-nav shadow on scroll, smooth anchor
// offset, and gentle reveal-on-scroll for sections. No build step, no deps.
(function () {
  "use strict";

  // Sticky nav border-on-scroll.
  const nav = document.querySelector(".nav");
  const onScroll = () => {
    if (!nav) return;
    if (window.scrollY > 8) nav.classList.add("scrolled");
    else nav.classList.remove("scrolled");
  };
  window.addEventListener("scroll", onScroll, { passive: true });
  onScroll();

  // Anchor offset for the sticky nav (CSS scroll-margin would also work,
  // but this keeps the offset in one place across browsers).
  document.querySelectorAll('a[href^="#"]').forEach((a) => {
    a.addEventListener("click", (e) => {
      const id = a.getAttribute("href").slice(1);
      if (!id) return;
      const el = document.getElementById(id);
      if (!el) return;
      e.preventDefault();
      const top = el.getBoundingClientRect().top + window.scrollY - 72;
      window.scrollTo({ top, behavior: "smooth" });
      history.replaceState(null, "", "#" + id);
    });
  });

  // Reveal sections on scroll (respects reduced-motion).
  if (
    window.matchMedia &&
    window.matchMedia("(prefers-reduced-motion: reduce)").matches
  ) {
    return;
  }
  const reveal = document.querySelectorAll(".section, .hero-asset");
  if (!("IntersectionObserver" in window)) return;
  const io = new IntersectionObserver(
    (entries) => {
      entries.forEach((en) => {
        if (en.isIntersecting) {
          en.target.style.opacity = "1";
          en.target.style.transform = "translateY(0)";
          io.unobserve(en.target);
        }
      });
    },
    { threshold: 0.08, rootMargin: "0px 0px -40px 0px" }
  );
  reveal.forEach((el) => {
    el.style.opacity = "0";
    el.style.transform = "translateY(16px)";
    el.style.transition =
      "opacity .5s cubic-bezier(.2,.7,.3,1), transform .5s cubic-bezier(.2,.7,.3,1)";
    el.style.willChange = "opacity, transform";
    io.observe(el);
  });
})();
