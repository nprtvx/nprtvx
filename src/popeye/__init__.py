# Styling and Script for Popeye Element

style = """
.popeye {
    background-color: #8926;
}
"""

script = """
const popeye = document.getElementById('popeye');
popeye.classList.add('popeye');
    // Include Babylon.js and the Babylon GUI libraries in your HTML for this to work.
    // Example: <script src="https://cdn.babylonjs.com/babylon.js"></script>
    // <script src="https://cdn.babylonjs.com/loaders/babylon.glTF2FileLoader.js"></script>

    const canvas = document.getElementById("renderCanvas"); // HTML canvas element
    const engine = new BABYLON.Engine(canvas, true);

    const createScene = async function () {
        const scene = new BABYLON.Scene(engine);
        scene.clearColor = new BABYLON.Color3.Black();

        // Camera
        const camera = new BABYLON.ArcRotateCamera("Camera", Math.PI / 3, Math.PI / 3, 20, BABYLON.Vector3.Zero(), scene);
        camera.attachControl(canvas, true);

        // Lighting
        const light = new BABYLON.PointLight("light", new BABYLON.Vector3(5, 10, -10), scene);
        light.intensity = 1;

        // Load Monkey Model (Suzanne GLB example, replace URL with your own model if needed)
        const monkey = await BABYLON.SceneLoader.ImportMeshAsync(
            "",
            "https://models.babylonjs.com/",
            "monkey.glb",
            scene
        );

        // Scale and position monkey
        monkey.meshes.scaling = new BABYLON.Vector3(2, 2, 2);
        monkey.meshes.position = new BABYLON.Vector3(0, 1, 0);

        // NEON MATERIAL
        const neonMaterial = new BABYLON.StandardMaterial("neonMat", scene);
        neonMaterial.emissiveColor = new BABYLON.Color3(0.1, 0.9, 1.0); // Cyan Glow

        // Advanced GlowLayer for strong neon effect
        const gl = new BABYLON.GlowLayer("glow", scene);
        gl.intensity = 0.8;

        // Create each letter as a 3D mesh
        const logoText = "neon monkey";
        const letters = [];
        const fontUrl = "https://assets.babylonjs.com/fonts/Droid Sans_Regular.json"; // Babylon-compatible font
        let xOffset = -6;

        for (let i = 0; i < logoText.length; i++) {
            if (logoText[i] === " ") {
                xOffset += 1.5;
                continue;
            }
            const letterMesh = BABYLON.MeshBuilder.CreateText(
                "letter" + i,
                { text: logoText[i], font: fontUrl, size: 2, depth: 0.4 },
                scene
            );
            letterMesh.material = neonMaterial;
            letterMesh.position = new BABYLON.Vector3(xOffset, 0, 0);
            letterMesh.metadata = { targetY: 4, startY: -3 };
            letterMesh.position.y = -3; // Start offscene, will animate up
            letters.push(letterMesh);
            xOffset += 1.3;
        }

        // Animate the monkey and logo construction
        let constructed = false;
        scene.registerBeforeRender(function () {
            if (!constructed) {
                let ready = true;
                for (let i = 0; i < letters.length; i++) {
                    if (letters[i].position.y < letters[i].metadata.targetY) {
                        letters[i].position.y += 0.1;
                    } else {
                        ready = false;
                    }
                }

                if (ready) {
                    constructed = true;
                }
            }
        });
    };

    // Create the scene and start the engine
    createScene();
    engine.runRenderLoop(function () {
        scene.render();
    });

    // Resize the canvas on window resize
    window.addEventListener("resize", function () {
        engine.resize();
    });
"""

popeye = """
<canvas id='renderCanvas'></canvas>
<script src="https://cdn.babylonjs.com/babylon.js"></script>
<script src="https://cdn.babylonjs.com/loaders/babylon.glTF2FileLoader.js"></script>
{{script}}
"""
