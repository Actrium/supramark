package main

import (
	"bytes"
	"encoding/json"
	"errors"
	"flag"
	"fmt"
	"go/ast"
	"go/parser"
	"go/printer"
	"go/token"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strconv"
	"strings"
)

type rawCase struct {
	Family             string
	Name               string
	Script             string
	Skip               bool
	JustDagre          bool
	TestSerialization  bool
	UseMeasuredTexts   bool
	DagreFeatureError  string
	ExpErr             string
	Source             string
}

type manifestCase struct {
	Family            string `json:"family"`
	Name              string `json:"name"`
	FixtureName       string `json:"fixture_name,omitempty"`
	Script            string `json:"script"`
	ExpectedKind      string `json:"expected_kind"`
	ExpectedMessage   string `json:"expected_message,omitempty"`
	SVGRelPath        string `json:"svg_relpath,omitempty"`
	ThemeID           int64  `json:"theme_id"`
	DarkThemeID       *int64 `json:"dark_theme_id,omitempty"`
	Sketch            bool   `json:"sketch"`
	UseMeasuredTexts  bool   `json:"use_measured_texts,omitempty"`
	TestSerialization bool   `json:"test_serialization,omitempty"`
	Source            string `json:"source,omitempty"`
}

type boardConfig struct {
	Config struct {
		Sketch      bool   `json:"sketch"`
		ThemeID     int64  `json:"themeID"`
		DarkThemeID *int64 `json:"darkThemeID"`
	} `json:"config"`
}

type env struct {
	fset         *token.FileSet
	e2eDir       string
	testMarkdown string
}

type archiveFile struct {
	Name string
	Data string
}

type familySpec struct {
	File   string
	Func   string
	Family string
	Mode   string
}

func main() {
	var (
		goE2EDir  = flag.String("go-e2e-dir", "/ext/d2/e2etests", "Path to upstream Go e2etests dir")
		fixtureDir = flag.String("fixture-dir", "tests/e2e_testdata", "Path to local e2e fixture dir")
		output    = flag.String("output", "tests/e2e_dagre_svg_cases.json", "Output manifest path")
	)
	flag.Parse()

	fset := token.NewFileSet()
	mdBytes, err := os.ReadFile(filepath.Join(*goE2EDir, "markdowntest.md"))
	if err != nil {
		die(err)
	}
	e := &env{
		fset:         fset,
		e2eDir:       *goE2EDir,
		testMarkdown: string(mdBytes),
	}

	specs := []familySpec{
		{File: "e2e_test.go", Func: "testSanity", Family: "sanity", Mode: "composite"},
		{File: "e2e_test.go", Func: "testTxtar", Family: "txtar", Mode: "txtar"},
		{File: "e2e_test.go", Func: "testASCIITxtar", Family: "asciitxtar", Mode: "asciitxtar"},
		{File: "stable_test.go", Func: "testStable", Family: "stable", Mode: "composite"},
		{File: "regression_test.go", Func: "testRegression", Family: "regression", Mode: "composite"},
		{File: "patterns_test.go", Func: "testPatterns", Family: "patterns", Mode: "composite"},
		{File: "todo_test.go", Func: "testTodo", Family: "todo", Mode: "composite"},
		{File: "measured_test.go", Func: "testMeasured", Family: "measured", Mode: "composite"},
		{File: "unicode_test.go", Func: "testUnicode", Family: "unicode", Mode: "composite"},
		{File: "root_test.go", Func: "testRoot", Family: "root", Mode: "composite"},
		{File: "themes_test.go", Func: "testThemes", Family: "themes", Mode: "composite"},
	}

	var rawCases []rawCase
	for _, spec := range specs {
		switch spec.Mode {
		case "composite":
			rcs, err := parseCompositeFamily(e, filepath.Join(*goE2EDir, spec.File), spec.Func, spec.Family)
			if err != nil {
				die(fmt.Errorf("%s/%s: %w", spec.File, spec.Func, err))
			}
			rawCases = append(rawCases, rcs...)
		case "txtar":
			files, err := parseArchive(filepath.Join(*goE2EDir, "txtar.txt"))
			if err != nil {
				die(err)
			}
			for _, f := range files {
				rawCases = append(rawCases, rawCase{
					Family: "txtar",
					Name:   f.Name,
					Script: f.Data,
					Source: "txtar.txt:" + f.Name,
				})
			}
		case "asciitxtar":
			files, err := parseArchive(filepath.Join(*goE2EDir, "asciitxtar.txt"))
			if err != nil {
				die(err)
			}
			for _, f := range files {
				rawCases = append(rawCases, rawCase{
					Family: "asciitxtar",
					Name:   f.Name,
					Script: f.Data,
					Source: "asciitxtar.txt:" + f.Name,
				})
			}
		default:
			die(fmt.Errorf("unknown mode %q", spec.Mode))
		}
	}

	manifest, extraFixtures, err := buildManifest(rawCases, *fixtureDir)
	if err != nil {
		die(err)
	}

	out, err := json.MarshalIndent(manifest, "", "  ")
	if err != nil {
		die(err)
	}
	out = append(out, '\n')
	if err := os.WriteFile(*output, out, 0o644); err != nil {
		die(err)
	}

	printSummary(manifest, extraFixtures)
}

func parseCompositeFamily(e *env, path, funcName, family string) ([]rawCase, error) {
	fileNode, err := parser.ParseFile(e.fset, path, nil, parser.ParseComments)
	if err != nil {
		return nil, err
	}
	var fn *ast.FuncDecl
	for _, decl := range fileNode.Decls {
		if d, ok := decl.(*ast.FuncDecl); ok && d.Name.Name == funcName {
			fn = d
			break
		}
	}
	if fn == nil {
		return nil, fmt.Errorf("function %s not found", funcName)
	}
	for _, stmt := range fn.Body.List {
		assign, ok := stmt.(*ast.AssignStmt)
		if !ok || len(assign.Rhs) != 1 {
			continue
		}
		lit, ok := assign.Rhs[0].(*ast.CompositeLit)
		if !ok {
			continue
		}
		if !isTestCaseSlice(lit.Type) {
			continue
		}
		var out []rawCase
		for _, elt := range lit.Elts {
			rc, include, err := evalCaseExpr(e, family, elt)
			if err != nil {
				return nil, err
			}
			if include {
				out = append(out, rc)
			}
		}
		return out, nil
	}
	return nil, errors.New("tcs slice not found")
}

func isTestCaseSlice(expr ast.Expr) bool {
	arr, ok := expr.(*ast.ArrayType)
	if !ok {
		return false
	}
	ident, ok := arr.Elt.(*ast.Ident)
	return ok && ident.Name == "testCase"
}

func evalCaseExpr(e *env, family string, expr ast.Expr) (rawCase, bool, error) {
	switch v := expr.(type) {
	case *ast.CompositeLit:
		rc, err := evalCaseComposite(e, family, v)
		return rc, true, err
	case *ast.CallExpr:
		return evalCaseCall(e, family, v)
	default:
		return rawCase{}, false, fmt.Errorf("unsupported case expr %T", expr)
	}
}

func evalCaseCall(e *env, family string, call *ast.CallExpr) (rawCase, bool, error) {
	fn, ok := call.Fun.(*ast.Ident)
	if !ok {
		return rawCase{}, false, fmt.Errorf("unsupported call fun %T", call.Fun)
	}
	switch fn.Name {
	case "loadFromFile":
		if len(call.Args) != 2 {
			return rawCase{}, false, fmt.Errorf("loadFromFile args=%d", len(call.Args))
		}
		name, err := evalStringExpr(e, call.Args[1])
		if err != nil {
			return rawCase{}, false, err
		}
		script, err := os.ReadFile(filepath.Join(e.e2eDir, "testdata", "files", name+".d2"))
		if err != nil {
			return rawCase{}, false, err
		}
		return rawCase{
			Family: family,
			Name:   name,
			Script: string(script),
			Source: posString(e.fset, call.Pos()),
		}, true, nil
	case "loadFromFileWithOptions":
		if len(call.Args) != 3 {
			return rawCase{}, false, fmt.Errorf("loadFromFileWithOptions args=%d", len(call.Args))
		}
		name, err := evalStringExpr(e, call.Args[1])
		if err != nil {
			return rawCase{}, false, err
		}
		lit, ok := call.Args[2].(*ast.CompositeLit)
		if !ok {
			return rawCase{}, false, fmt.Errorf("loadFromFileWithOptions options %T", call.Args[2])
		}
		rc, err := evalCaseComposite(e, family, lit)
		if err != nil {
			return rawCase{}, false, err
		}
		script, err := os.ReadFile(filepath.Join(e.e2eDir, "testdata", "files", name+".d2"))
		if err != nil {
			return rawCase{}, false, err
		}
		rc.Name = name
		rc.Script = string(script)
		rc.Source = posString(e.fset, call.Pos())
		return rc, true, nil
	default:
		return rawCase{}, false, fmt.Errorf("unsupported call %s", fn.Name)
	}
}

func evalCaseComposite(e *env, family string, lit *ast.CompositeLit) (rawCase, error) {
	rc := rawCase{
		Family: family,
		Source: posString(e.fset, lit.Pos()),
	}
	for _, elt := range lit.Elts {
		kv, ok := elt.(*ast.KeyValueExpr)
		if !ok {
			return rawCase{}, fmt.Errorf("unexpected composite elt %T", elt)
		}
		key, ok := kv.Key.(*ast.Ident)
		if !ok {
			return rawCase{}, fmt.Errorf("unexpected field key %T", kv.Key)
		}
		switch key.Name {
		case "name":
			s, err := evalStringExpr(e, kv.Value)
			if err != nil {
				return rawCase{}, err
			}
			rc.Name = s
		case "script":
			s, err := evalStringExpr(e, kv.Value)
			if err != nil {
				return rawCase{}, err
			}
			rc.Script = s
		case "skip":
			b, err := evalBoolExpr(kv.Value)
			if err != nil {
				return rawCase{}, err
			}
			rc.Skip = b
		case "justDagre":
			b, err := evalBoolExpr(kv.Value)
			if err != nil {
				return rawCase{}, err
			}
			rc.JustDagre = b
		case "testSerialization":
			b, err := evalBoolExpr(kv.Value)
			if err != nil {
				return rawCase{}, err
			}
			rc.TestSerialization = b
		case "mtexts":
			rc.UseMeasuredTexts = true
		case "dagreFeatureError":
			s, err := evalStringExpr(e, kv.Value)
			if err != nil {
				return rawCase{}, err
			}
			rc.DagreFeatureError = s
		case "expErr":
			s, err := evalStringExpr(e, kv.Value)
			if err != nil {
				return rawCase{}, err
			}
			rc.ExpErr = s
		case "themeID", "elkFeatureError", "assertions":
			// Theme is derived from board.exp.json when a dagre fixture exists.
			// ELK-only fields and assertion callbacks are intentionally ignored.
		default:
			return rawCase{}, fmt.Errorf("unsupported field %s in %s", key.Name, rc.Source)
		}
	}
	return rc, nil
}

func evalBoolExpr(expr ast.Expr) (bool, error) {
	id, ok := expr.(*ast.Ident)
	if !ok {
		return false, fmt.Errorf("unsupported bool expr %T", expr)
	}
	switch id.Name {
	case "true":
		return true, nil
	case "false":
		return false, nil
	default:
		return false, fmt.Errorf("unsupported bool ident %q", id.Name)
	}
}

func evalStringExpr(e *env, expr ast.Expr) (string, error) {
	switch v := expr.(type) {
	case *ast.BasicLit:
		if v.Kind != token.STRING {
			return "", fmt.Errorf("unsupported literal kind %s", v.Kind)
		}
		return strconv.Unquote(v.Value)
	case *ast.Ident:
		switch v.Name {
		case "testMarkdown":
			return e.testMarkdown, nil
		default:
			return "", fmt.Errorf("unsupported ident %q", v.Name)
		}
	case *ast.CallExpr:
		fn, ok := v.Fun.(*ast.Ident)
		if !ok {
			return "", fmt.Errorf("unsupported call fun %T", v.Fun)
		}
		switch fn.Name {
		case "mdTestScript":
			if len(v.Args) != 1 {
				return "", fmt.Errorf("mdTestScript args=%d", len(v.Args))
			}
			md, err := evalStringExpr(e, v.Args[0])
			if err != nil {
				return "", err
			}
			return fmt.Sprintf("\nmd: |md\n%s\n|\na -> md -> b\n", md), nil
		default:
			return "", fmt.Errorf("unsupported string call %s", fn.Name)
		}
	case *ast.BinaryExpr:
		if v.Op != token.ADD {
			return "", fmt.Errorf("unsupported string binary op %s", v.Op)
		}
		left, err := evalStringExpr(e, v.X)
		if err != nil {
			return "", err
		}
		right, err := evalStringExpr(e, v.Y)
		if err != nil {
			return "", err
		}
		return left + right, nil
	default:
		return "", fmt.Errorf("unsupported string expr %T (%s)", expr, exprString(e.fset, expr))
	}
}

func buildManifest(rawCases []rawCase, fixtureRoot string) ([]manifestCase, []string, error) {
	seen := map[string]int{}
	var out []manifestCase
	referencedFixtures := map[string]bool{}

	for _, rc := range rawCases {
		if rc.Skip {
			continue
		}
		key := rc.Family + "\x00" + rc.Name
		occ := seen[key]
		seen[key]++

		fixtureName := canonicalFixtureName(rc.Name, occ)
		svgRel := fixtureSVGRelPath(rc.Family, fixtureName)
		svgAbs := filepath.Join(fixtureRoot, svgRel)
		boardRel := fixtureBoardRelPath(rc.Family, fixtureName)
		boardAbs := filepath.Join(fixtureRoot, boardRel)

		var (
			expectedKind    string
			expectedMessage string
			themeID         int64
			darkThemeID     *int64
			sketch          bool
		)

		if fileExists(svgAbs) {
			expectedKind = "svg"
			referencedFixtures[svgRel] = true
			if boardRel != "" && fileExists(boardAbs) {
				referencedFixtures[boardRel] = true
				cfg, err := readBoardConfig(boardAbs)
				if err != nil {
					return nil, nil, err
				}
				themeID = cfg.Config.ThemeID
				darkThemeID = cfg.Config.DarkThemeID
				sketch = cfg.Config.Sketch
			}
		} else if rc.DagreFeatureError != "" {
			expectedKind = "dagre_feature_error"
			expectedMessage = rc.DagreFeatureError
		} else if rc.ExpErr != "" {
			expectedKind = "compile_error"
			expectedMessage = rc.ExpErr
		} else {
			continue
		}

		out = append(out, manifestCase{
			Family:            rc.Family,
			Name:              rc.Name,
			FixtureName:       fixtureName,
			Script:            rc.Script,
			ExpectedKind:      expectedKind,
			ExpectedMessage:   expectedMessage,
			SVGRelPath:        svgRelIfAny(expectedKind, svgRel),
			ThemeID:           themeID,
			DarkThemeID:       darkThemeID,
			Sketch:            sketch,
			UseMeasuredTexts:  rc.UseMeasuredTexts,
			TestSerialization: rc.TestSerialization,
			Source:            rc.Source,
		})
	}

	extraFixtures, err := findUnreferencedFixtures(fixtureRoot, referencedFixtures)
	if err != nil {
		return nil, nil, err
	}
	return out, extraFixtures, nil
}

func svgRelIfAny(expectedKind, rel string) string {
	if expectedKind == "svg" {
		return rel
	}
	return ""
}

func canonicalFixtureName(name string, occurrence int) string {
	base := strings.ReplaceAll(name, " ", "_")
	if occurrence == 0 {
		return base
	}
	return fmt.Sprintf("%s#%02d", base, occurrence)
}

func fixtureSVGRelPath(family, fixtureName string) string {
	if family == "asciitxtar" {
		return filepath.ToSlash(filepath.Join(family, fixtureName, "sketch.exp.svg"))
	}
	return filepath.ToSlash(filepath.Join(family, fixtureName, "dagre", "sketch.exp.svg"))
}

func fixtureBoardRelPath(family, fixtureName string) string {
	if family == "asciitxtar" {
		return ""
	}
	return filepath.ToSlash(filepath.Join(family, fixtureName, "dagre", "board.exp.json"))
}

func readBoardConfig(path string) (boardConfig, error) {
	var cfg boardConfig
	data, err := os.ReadFile(path)
	if err != nil {
		return cfg, err
	}
	if err := json.Unmarshal(data, &cfg); err != nil {
		return cfg, fmt.Errorf("%s: %w", path, err)
	}
	return cfg, nil
}

func parseArchive(path string) ([]archiveFile, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}
	re := regexp.MustCompile(`(?m)^-- ([^\n]+) --\n`)
	matches := re.FindAllSubmatchIndex(data, -1)
	if len(matches) == 0 {
		return nil, fmt.Errorf("%s: no archive entries found", path)
	}
	var out []archiveFile
	for i, m := range matches {
		name := string(data[m[2]:m[3]])
		start := m[1]
		end := len(data)
		if i+1 < len(matches) {
			end = matches[i+1][0]
		}
		out = append(out, archiveFile{
			Name: name,
			Data: string(data[start:end]),
		})
	}
	return out, nil
}

func findUnreferencedFixtures(fixtureRoot string, referenced map[string]bool) ([]string, error) {
	var extras []string
	err := filepath.Walk(fixtureRoot, func(path string, info os.FileInfo, err error) error {
		if err != nil {
			return err
		}
		if info.IsDir() {
			return nil
		}
		rel, err := filepath.Rel(fixtureRoot, path)
		if err != nil {
			return err
		}
		rel = filepath.ToSlash(rel)
		isDagreSVG := strings.HasSuffix(rel, "/dagre/sketch.exp.svg")
		isASCIIFixture := strings.HasPrefix(rel, "asciitxtar/") && strings.HasSuffix(rel, "/sketch.exp.svg")
		if isDagreSVG || isASCIIFixture {
			if !referenced[rel] {
				extras = append(extras, rel)
			}
		}
		return nil
	})
	if err != nil {
		return nil, err
	}
	sort.Strings(extras)
	return extras, nil
}

func printSummary(manifest []manifestCase, extraFixtures []string) {
	counts := map[string]int{}
	kinds := map[string]int{}
	for _, c := range manifest {
		counts[c.Family]++
		kinds[c.ExpectedKind]++
	}
	fmt.Fprintf(os.Stderr, "generated %d applicable dagre cases\n", len(manifest))
	var families []string
	for fam := range counts {
		families = append(families, fam)
	}
	sort.Strings(families)
	for _, fam := range families {
		fmt.Fprintf(os.Stderr, "  %s: %d\n", fam, counts[fam])
	}
	var kindNames []string
	for k := range kinds {
		kindNames = append(kindNames, k)
	}
	sort.Strings(kindNames)
	for _, k := range kindNames {
		fmt.Fprintf(os.Stderr, "  kind %s: %d\n", k, kinds[k])
	}
	if len(extraFixtures) > 0 {
		fmt.Fprintf(os.Stderr, "unreferenced dagre fixtures: %d\n", len(extraFixtures))
		for _, rel := range extraFixtures {
			fmt.Fprintf(os.Stderr, "  %s\n", rel)
		}
	}
}

func posString(fset *token.FileSet, pos token.Pos) string {
	p := fset.Position(pos)
	return fmt.Sprintf("%s:%d", filepath.Base(p.Filename), p.Line)
}

func exprString(fset *token.FileSet, expr ast.Expr) string {
	var buf bytes.Buffer
	_ = printer.Fprint(&buf, fset, expr)
	return buf.String()
}

func fileExists(path string) bool {
	if path == "" {
		return false
	}
	_, err := os.Stat(path)
	return err == nil
}

func die(err error) {
	fmt.Fprintln(os.Stderr, "error:", err)
	os.Exit(1)
}
