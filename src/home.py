home = f"""
<div class="nprtvx">
<h1 id="nxtitle">nprtvx</h1>
<div class="section-nm">

</div>
"""

style = f"""
.nprtvx {{
position: absolute;
top: 0;
bottom: 0;
left: 0;
right: 0;
background-image: linear-gradient(26deg, black, white);
}}
"""

script = f"""
const nxtitle = document.getElementById("nxtitle");
nxtitle.addEventListener("click", (event) {{
console.log(event);
location.href = "/";
}});
"""