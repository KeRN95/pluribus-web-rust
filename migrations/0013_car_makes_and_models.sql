-- The makes and models sellers pick from, for cars and for the parts that fit them: the makes common
-- in Albania, each with its usual models, instead of the five makes and two models each that the
-- sample data had. A make or model a seller typed in ("Tjetër", migration 0009) that is now on the
-- list joins it, approved and spelt the list's way, and its listings keep it: "s-class" becomes
-- S-Class. Codes are made the way the application makes a typed option's (format::slug, upper case,
-- a model's after its make's), so the ones already in use stay: VOLKSWAGEN_GOLF, BMW_3_SERIES.

CREATE FUNCTION pg_temp.option_code(name text) RETURNS text
LANGUAGE sql IMMUTABLE
AS $$
    SELECT upper(replace(trim(BOTH '-' FROM regexp_replace(
               translate(lower(name), 'ëéèêçćčáàâäóòôöúùûüíìîïšž', 'eeeecccaaaaoooouuuuiiiisz'),
               '[^a-z0-9]+', '-', 'g')), '-', '_'))
$$;

-- Each make with its models, in the order the lists show them.
CREATE TEMPORARY TABLE car_makes (make text NOT NULL, models text[] NOT NULL) ON COMMIT DROP;
INSERT INTO car_makes (make, models) VALUES
('Alfa Romeo', ARRAY['145', '146', '147', '156', '159', '166', 'Brera', 'Giulia', 'Giulietta', 'GT', 'MiTo', 'Spider', 'Stelvio', 'Tonale']),
('Audi', ARRAY['80', '100', 'A1', 'A2', 'A3', 'A4', 'A5', 'A6', 'A7', 'A8', 'e-tron', 'Q2', 'Q3', 'Q4 e-tron', 'Q5', 'Q7', 'Q8', 'R8', 'TT']),
('BMW', ARRAY['1 Series', '2 Series', '3 Series', '4 Series', '5 Series', '6 Series', '7 Series', '8 Series', 'i3', 'i4', 'iX', 'X1', 'X2', 'X3', 'X4', 'X5', 'X6', 'X7', 'Z3', 'Z4']),
('BYD', ARRAY['Atto 3', 'Dolphin', 'Han', 'Seal', 'Seal U', 'Tang']),
('Chevrolet', ARRAY['Aveo', 'Camaro', 'Captiva', 'Cruze', 'Epica', 'Kalos', 'Lacetti', 'Malibu', 'Matiz', 'Orlando', 'Spark', 'Trax']),
('Chrysler', ARRAY['300C', 'Grand Voyager', 'PT Cruiser', 'Sebring', 'Voyager']),
('Citroën', ARRAY['Berlingo', 'C1', 'C2', 'C3', 'C3 Aircross', 'C3 Picasso', 'C4', 'C4 Cactus', 'C4 Picasso', 'C5', 'C5 Aircross', 'C8', 'C-Elysée', 'Jumper', 'Jumpy', 'Nemo', 'Saxo', 'Xantia', 'Xsara', 'Xsara Picasso']),
('Dacia', ARRAY['Dokker', 'Duster', 'Jogger', 'Lodgy', 'Logan', 'Sandero', 'Spring']),
('Daewoo', ARRAY['Espero', 'Kalos', 'Lanos', 'Leganza', 'Matiz', 'Nexia', 'Nubira', 'Tico']),
('Dodge', ARRAY['Caliber', 'Challenger', 'Charger', 'Durango', 'Journey', 'Nitro']),
('Fiat', ARRAY['500', '500L', '500X', 'Albea', 'Brava', 'Bravo', 'Croma', 'Doblò', 'Ducato', 'Fiorino', 'Freemont', 'Grande Punto', 'Idea', 'Linea', 'Marea', 'Multipla', 'Palio', 'Panda', 'Punto', 'Qubo', 'Scudo', 'Sedici', 'Seicento', 'Stilo', 'Tipo', 'Uno']),
('Ford', ARRAY['B-Max', 'C-Max', 'EcoSport', 'Edge', 'Escort', 'Explorer', 'Fiesta', 'Focus', 'Fusion', 'Galaxy', 'Ka', 'Kuga', 'Mondeo', 'Mustang', 'Mustang Mach-E', 'Puma', 'Ranger', 'S-Max', 'Sierra', 'Tourneo', 'Transit']),
('Honda', ARRAY['Accord', 'City', 'Civic', 'CR-V', 'HR-V', 'Insight', 'Jazz', 'Legend', 'Prelude']),
('Hyundai', ARRAY['Accent', 'Atos', 'Bayon', 'Elantra', 'Getz', 'H-1', 'i10', 'i20', 'i30', 'i40', 'Ioniq', 'Ioniq 5', 'Ioniq 6', 'ix20', 'ix35', 'Kona', 'Matrix', 'Santa Fe', 'Sonata', 'Terracan', 'Trajet', 'Tucson']),
('Iveco', ARRAY['Daily', 'Eurocargo']),
('Jaguar', ARRAY['E-Pace', 'F-Pace', 'F-Type', 'I-Pace', 'S-Type', 'X-Type', 'XE', 'XF', 'XJ']),
('Jeep', ARRAY['Avenger', 'Cherokee', 'Commander', 'Compass', 'Gladiator', 'Grand Cherokee', 'Patriot', 'Renegade', 'Wrangler']),
('Kia', ARRAY['Carens', 'Carnival', 'Ceed', 'Cerato', 'EV6', 'Niro', 'Optima', 'Picanto', 'ProCeed', 'Rio', 'Sorento', 'Soul', 'Sportage', 'Stinger', 'Stonic', 'Venga', 'XCeed']),
('Lada', ARRAY['2107', 'Granta', 'Kalina', 'Niva', 'Samara', 'Vesta']),
('Lancia', ARRAY['Delta', 'Kappa', 'Lybra', 'Musa', 'Phedra', 'Thesis', 'Ypsilon']),
('Land Rover', ARRAY['Defender', 'Discovery', 'Discovery Sport', 'Freelander', 'Range Rover', 'Range Rover Evoque', 'Range Rover Sport', 'Range Rover Velar']),
('Lexus', ARRAY['CT', 'ES', 'GS', 'IS', 'LS', 'LX', 'NX', 'RX', 'UX']),
('Mazda', ARRAY['2', '3', '5', '6', '323', '626', 'CX-3', 'CX-5', 'CX-7', 'CX-9', 'CX-30', 'CX-60', 'MX-5', 'Premacy', 'RX-8', 'Tribute']),
('Mercedes-Benz', ARRAY['190', 'A-Class', 'B-Class', 'C-Class', 'Citan', 'CLA', 'CLK', 'CLS', 'E-Class', 'EQA', 'EQB', 'EQC', 'EQE', 'EQS', 'G-Class', 'GL', 'GLA', 'GLB', 'GLC', 'GLE', 'GLK', 'GLS', 'ML', 'R-Class', 'S-Class', 'SL', 'SLK', 'Sprinter', 'V-Class', 'Vaneo', 'Viano', 'Vito', 'X-Class']),
('MG', ARRAY['HS', 'MG3', 'MG4', 'MG5', 'ZS']),
('Mini', ARRAY['Cabrio', 'Clubman', 'Cooper', 'Countryman', 'One', 'Paceman']),
('Mitsubishi', ARRAY['ASX', 'Carisma', 'Colt', 'Eclipse Cross', 'Galant', 'L200', 'Lancer', 'Outlander', 'Pajero', 'Pajero Sport', 'Space Star']),
('Nissan', ARRAY['350Z', '370Z', 'Almera', 'Ariya', 'Juke', 'Leaf', 'Micra', 'Murano', 'Navara', 'Note', 'Pathfinder', 'Patrol', 'Primera', 'Pulsar', 'Qashqai', 'Sunny', 'Terrano', 'Tiida', 'X-Trail']),
('Opel', ARRAY['Adam', 'Agila', 'Ampera', 'Antara', 'Astra', 'Calibra', 'Combo', 'Corsa', 'Crossland', 'Frontera', 'Grandland', 'Insignia', 'Kadett', 'Karl', 'Meriva', 'Mokka', 'Movano', 'Omega', 'Signum', 'Tigra', 'Vectra', 'Vivaro', 'Zafira']),
('Peugeot', ARRAY['106', '107', '108', '205', '206', '207', '208', '301', '306', '307', '308', '406', '407', '408', '508', '607', '807', '2008', '3008', '5008', 'Bipper', 'Boxer', 'Expert', 'Partner', 'RCZ', 'Rifter']),
('Porsche', ARRAY['911', 'Boxster', 'Cayenne', 'Cayman', 'Macan', 'Panamera', 'Taycan']),
('Renault', ARRAY['Arkana', 'Austral', 'Captur', 'Clio', 'Espace', 'Fluence', 'Kadjar', 'Kangoo', 'Koleos', 'Laguna', 'Master', 'Megane', 'Modus', 'Scenic', 'Symbol', 'Talisman', 'Thalia', 'Trafic', 'Twingo', 'Zoe']),
('Seat', ARRAY['Alhambra', 'Altea', 'Arona', 'Ateca', 'Cordoba', 'Exeo', 'Ibiza', 'Leon', 'Mii', 'Tarraco', 'Toledo']),
('Skoda', ARRAY['Citigo', 'Enyaq', 'Fabia', 'Felicia', 'Kamiq', 'Karoq', 'Kodiaq', 'Octavia', 'Rapid', 'Roomster', 'Scala', 'Superb', 'Yeti']),
('Smart', ARRAY['Forfour', 'Fortwo']),
('SsangYong', ARRAY['Actyon', 'Korando', 'Kyron', 'Rexton', 'Tivoli']),
('Subaru', ARRAY['BRZ', 'Forester', 'Impreza', 'Legacy', 'Outback', 'XV']),
('Suzuki', ARRAY['Alto', 'Baleno', 'Celerio', 'Grand Vitara', 'Ignis', 'Jimny', 'Liana', 'S-Cross', 'Splash', 'Swift', 'SX4', 'Vitara', 'Wagon R']),
('Tesla', ARRAY['Model 3', 'Model S', 'Model X', 'Model Y']),
('Toyota', ARRAY['Auris', 'Avensis', 'Aygo', 'C-HR', 'Camry', 'Carina', 'Celica', 'Corolla', 'Corolla Verso', 'GT86', 'Highlander', 'Hilux', 'Land Cruiser', 'Previa', 'Prius', 'Proace', 'RAV4', 'Starlet', 'Supra', 'Urban Cruiser', 'Verso', 'Yaris', 'Yaris Cross']),
('Volkswagen', ARRAY['Amarok', 'Arteon', 'Beetle', 'Bora', 'Caddy', 'Corrado', 'Crafter', 'Eos', 'Fox', 'Golf', 'Golf Plus', 'Golf Sportsvan', 'ID.3', 'ID.4', 'ID.5', 'ID. Buzz', 'Jetta', 'Lupo', 'Multivan', 'Passat', 'Passat CC', 'Phaeton', 'Polo', 'Scirocco', 'Sharan', 'T-Cross', 'T-Roc', 'Taigo', 'Tiguan', 'Touareg', 'Touran', 'Transporter', 'Up!', 'Vento']),
('Volvo', ARRAY['C30', 'C40', 'C70', 'EX30', 'S40', 'S60', 'S80', 'S90', 'V40', 'V50', 'V60', 'V70', 'V90', 'XC40', 'XC60', 'XC70', 'XC90']);

-- The makes, in alphabetical order. On a code already there, the list's name, place and approval win.
INSERT INTO attribute_options (attribute_id, value_code, label, sort_order)
SELECT a.id, pg_temp.option_code(m.make), m.make, 10 * row_number() OVER (ORDER BY lower(m.make))
FROM car_makes m
JOIN attributes a ON a.code = 'vehicle_make'
ON CONFLICT (attribute_id, value_code) DO UPDATE
SET label = EXCLUDED.label, sort_order = EXCLUDED.sort_order, active = true;

-- The models, under their make.
INSERT INTO attribute_options (attribute_id, parent_option_id, value_code, label, sort_order)
SELECT model_attribute.id, make.id, make.value_code || '_' || pg_temp.option_code(model.label), model.label,
       10 * model.position
FROM car_makes m
CROSS JOIN LATERAL unnest(m.models) WITH ORDINALITY AS model(label, position)
JOIN attributes make_attribute ON make_attribute.code = 'vehicle_make'
JOIN attribute_options make ON make.attribute_id = make_attribute.id AND make.value_code = pg_temp.option_code(m.make)
JOIN attributes model_attribute ON model_attribute.code = 'vehicle_model'
ON CONFLICT (attribute_id, value_code) DO UPDATE
SET label = EXCLUDED.label, parent_option_id = EXCLUDED.parent_option_id, sort_order = EXCLUDED.sort_order,
    active = true;

DO $$
DECLARE
    make_count integer;
    model_count integer;
BEGIN
    -- Every make and model on the list is offered, under its own code.
    SELECT count(*), sum(cardinality(models)) INTO make_count, model_count FROM car_makes;
    IF (SELECT count(*) FROM car_makes m
        JOIN attribute_options o ON o.value_code = pg_temp.option_code(m.make) AND o.active
        JOIN attributes a ON a.id = o.attribute_id AND a.code = 'vehicle_make') <> make_count THEN
        RAISE EXCEPTION 'expected % makes on the list', make_count;
    END IF;
    IF (SELECT count(*) FROM car_makes m
        CROSS JOIN LATERAL unnest(m.models) AS model(label)
        JOIN attribute_options o
          ON o.value_code = pg_temp.option_code(m.make) || '_' || pg_temp.option_code(model.label) AND o.active
        JOIN attributes a ON a.id = o.attribute_id AND a.code = 'vehicle_model') <> model_count THEN
        RAISE EXCEPTION 'expected % models on the list', model_count;
    END IF;
    -- The codes listings already use are still there, under the same make.
    IF (SELECT count(*) FROM attribute_options model
        JOIN attribute_options make ON make.id = model.parent_option_id
        WHERE (make.value_code, model.value_code) IN (('VOLKSWAGEN', 'VOLKSWAGEN_GOLF'), ('BMW', 'BMW_3_SERIES'),
                                                      ('MERCEDES_BENZ', 'MERCEDES_BENZ_C_CLASS'), ('AUDI', 'AUDI_A4'),
                                                      ('TOYOTA', 'TOYOTA_YARIS'))) <> 5 THEN
        RAISE EXCEPTION 'expected the models already in use to keep their codes';
    END IF;
END
$$;
