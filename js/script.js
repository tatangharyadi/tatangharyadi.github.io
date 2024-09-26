const toggleBtn = document.querySelector('.toggle-btn');
const toggleBtnIcon = document.querySelector('.toggle-btn i');
const navDropdown = document.querySelector('.nav-dropdown');

toggleBtn.onclick = function () {
    navDropdown.classList.toggle('open');
}
